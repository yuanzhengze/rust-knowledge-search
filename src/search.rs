//! 搜索排序模块 —— 把查询字符串变成排序后的 [`SearchResult`] 列表。
//!
//! 负责人:邱俊杰(见 `agent.md` §10、`plan.md` Day 6)。
//!
//! 设计要点(见 `docs/接口设计.md` §8):
//! - 空查询 / 仅空白 / `top_k == 0` 返回 [`AppError::Index`]。
//! - 无结果时返回空 `Vec`,不应崩溃。
//! - 排序逻辑是纯函数(输入 = `index + chunks + query`),便于单测。
//!
//! 评分策略(MVP):
//! - 用 [`crate::indexer::tokenize`] 拆 query,保证与索引侧规则一致。
//! - 对每个 query term 在 [`InvertedIndex::term_freq`] 中累加 chunk 的命中频次,
//!   作为评分。后续可平滑升级到 TF-IDF / BM25(分母只需新增 `doc_count` 等字段)。
//! - 同分时按 `chunk_id` 升序稳定排序,保证测试可复现。

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::chunker::DocumentChunk;
use crate::error::{AppError, AppResult};
use crate::indexer::{tokenize, InvertedIndex};

/// 单条搜索结果 —— 见 `docs/接口设计.md` §2.4。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 命中片段的 ID。
    pub chunk_id: String,
    /// 命中片段所属文件。
    pub file_path: PathBuf,
    /// 用于命令行展示的摘要片段(可截断,可高亮)。
    pub snippet: String,
    /// 相关度分数,越大越相关。
    pub score: f64,
    /// 命中的关键词列表,用于 CLI 高亮。
    pub matched_terms: Vec<String>,
}

/// 摘要长度上限(以 char 计,避免 UTF-8 截断中文字符)。
/// 与 plan.md Day 9 的"优化结果片段长度"保持单点配置,后续要改在此处调整。
const SNIPPET_MAX_CHARS: usize = 120;

/// 在索引上执行查询并返回 Top K 结果。
///
/// 行为约定见模块级文档。Tie-break 规则:分数降序,同分按 `chunk_id` 升序。
pub fn search(
    index: &InvertedIndex,
    chunks: &[DocumentChunk],
    query: &str,
    top_k: usize,
) -> AppResult<Vec<SearchResult>> {
    if query.trim().is_empty() {
        return Err(AppError::Index("empty query".into()));
    }
    if top_k == 0 {
        return Err(AppError::Index("top_k must be greater than zero".into()));
    }

    let query_terms = tokenize(query);
    if query_terms.is_empty() {
        return Err(AppError::Index(
            "query contains no searchable tokens".into(),
        ));
    }

    // 累加分数 + 收集命中关键词。BTreeSet 保证 matched_terms 输出顺序稳定。
    let mut scores: HashMap<&str, f64> = HashMap::new();
    let mut matched: HashMap<&str, BTreeSet<String>> = HashMap::new();

    for term in &query_terms {
        let Some(per_chunk) = index.term_freq.get(term) else {
            continue;
        };
        for (chunk_id, freq) in per_chunk {
            *scores.entry(chunk_id.as_str()).or_insert(0.0) += *freq as f64;
            matched
                .entry(chunk_id.as_str())
                .or_default()
                .insert(term.clone());
        }
    }

    if scores.is_empty() {
        return Ok(Vec::new());
    }

    let chunk_lookup: HashMap<&str, &DocumentChunk> =
        chunks.iter().map(|c| (c.id.as_str(), c)).collect();

    let mut ranked: Vec<(&DocumentChunk, f64)> = scores
        .into_iter()
        .filter_map(|(id, score)| chunk_lookup.get(id).map(|c| (*c, score)))
        .collect();

    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.0.id.cmp(&b.0.id))
    });
    ranked.truncate(top_k);

    let results = ranked
        .into_iter()
        .map(|(chunk, score)| {
            let matched_terms = matched
                .get(chunk.id.as_str())
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();
            SearchResult {
                chunk_id: chunk.id.clone(),
                file_path: chunk.file_path.clone(),
                snippet: make_snippet(&chunk.content),
                score,
                matched_terms,
            }
        })
        .collect();

    Ok(results)
}

/// 生成摘要 —— 当前实现:取前 N 个 char,过长时追加省略号。
///
/// 后续 Day 9 可以扩展为"围绕首个匹配位置取上下文 + 关键词高亮",
/// 但 SNIPPET_MAX_CHARS 这个上限点保持不变。
fn make_snippet(content: &str) -> String {
    let trimmed = content.trim();
    let total = trimmed.chars().count();
    if total <= SNIPPET_MAX_CHARS {
        return trimmed.to_string();
    }
    let mut snippet: String = trimmed.chars().take(SNIPPET_MAX_CHARS).collect();
    snippet.push('…');
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &str, content: &str) -> DocumentChunk {
        DocumentChunk {
            id: id.to_string(),
            file_path: PathBuf::from(format!("/virtual/{id}.md")),
            title: None,
            content: content.to_string(),
            start_line: 1,
            end_line: 1,
        }
    }

    fn build(chunks: &[DocumentChunk]) -> InvertedIndex {
        InvertedIndex::build(chunks)
    }

    #[test]
    fn search_should_return_ranked_results() {
        let chunks = vec![
            chunk("c1", "Rust 所有权 是 Rust 的核心"),
            chunk("c2", "Rust 错误处理"),
            chunk("c3", "Python 装饰器"),
        ];
        let index = build(&chunks);

        let results = search(&index, &chunks, "Rust", 10).expect("search ok");
        assert_eq!(results.len(), 2, "Python chunk 不应命中 Rust");
        // c1 出现两次 Rust,c2 出现一次,排序应当是 c1 在前
        assert_eq!(results[0].chunk_id, "c1");
        assert_eq!(results[1].chunk_id, "c2");
        assert!(results[0].score > results[1].score);
        assert_eq!(results[0].matched_terms, vec!["rust".to_string()]);
    }

    #[test]
    fn search_should_respect_top_k() {
        let chunks = vec![
            chunk("c1", "Rust"),
            chunk("c2", "Rust"),
            chunk("c3", "Rust"),
            chunk("c4", "Rust"),
        ];
        let index = build(&chunks);

        let results = search(&index, &chunks, "Rust", 2).expect("search ok");
        assert_eq!(results.len(), 2);
        // 同分按 chunk_id 升序
        assert_eq!(results[0].chunk_id, "c1");
        assert_eq!(results[1].chunk_id, "c2");
    }

    #[test]
    fn search_should_reject_empty_query() {
        let index = build(&[chunk("c1", "Rust")]);
        let chunks = vec![chunk("c1", "Rust")];

        assert!(matches!(
            search(&index, &chunks, "", 5),
            Err(AppError::Index(_))
        ));
        assert!(matches!(
            search(&index, &chunks, "   \t\n", 5),
            Err(AppError::Index(_))
        ));
    }

    #[test]
    fn search_should_reject_query_with_only_punctuation() {
        let index = build(&[chunk("c1", "Rust")]);
        let chunks = vec![chunk("c1", "Rust")];

        assert!(matches!(
            search(&index, &chunks, ",,,。!?", 5),
            Err(AppError::Index(_))
        ));
    }

    #[test]
    fn search_should_reject_zero_top_k() {
        let index = build(&[chunk("c1", "Rust")]);
        let chunks = vec![chunk("c1", "Rust")];

        assert!(matches!(
            search(&index, &chunks, "Rust", 0),
            Err(AppError::Index(_))
        ));
    }

    #[test]
    fn search_should_return_empty_for_no_match() {
        let chunks = vec![chunk("c1", "Rust 所有权")];
        let index = build(&chunks);

        let results = search(&index, &chunks, "kotlin", 5).expect("search ok");
        assert!(results.is_empty());
    }

    #[test]
    fn search_should_handle_multi_term_query() {
        let chunks = vec![
            chunk("c1", "Rust 所有权 错误处理"), // 命中 rust + 所 + 有 + 权
            chunk("c2", "Rust 基础"),            // 仅命中 rust
            chunk("c3", "所有权"),               // 仅命中 所 + 有 + 权
        ];
        let index = build(&chunks);

        let results = search(&index, &chunks, "Rust 所有权", 5).expect("search ok");
        assert_eq!(results.len(), 3);
        // c1 命中 4 个 token (rust + 所 + 有 + 权),分最高
        assert_eq!(results[0].chunk_id, "c1");
        assert!(results[0].score >= results[1].score);
        assert!(results[0].matched_terms.len() >= 2);
    }

    #[test]
    fn search_should_match_chinese_query() {
        let chunks = vec![
            chunk("c1", "Rust 所有权 借用 生命周期"),
            chunk("c2", "Rust 错误处理 Result"),
        ];
        let index = build(&chunks);

        let results = search(&index, &chunks, "所有权", 5).expect("search ok");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "c1");
        // 三个 CJK 字符都命中
        assert_eq!(results[0].score, 3.0);
    }

    #[test]
    fn search_should_truncate_long_snippets() {
        let long = "a".repeat(500);
        let chunks = vec![chunk("c1", &format!("Rust {long}"))];
        let index = build(&chunks);

        let results = search(&index, &chunks, "Rust", 5).expect("search ok");
        assert_eq!(results.len(), 1);
        let snippet = &results[0].snippet;
        let char_count = snippet.chars().count();
        assert!(
            char_count <= SNIPPET_MAX_CHARS + 1,
            "snippet 字符数应受 SNIPPET_MAX_CHARS 约束,实际 {char_count}"
        );
        assert!(snippet.ends_with('…'));
    }

    #[test]
    fn search_should_skip_chunks_missing_from_chunk_set() {
        // 索引内有 c1 c2,但传入的 chunks 切片只剩 c2 —— 模拟 chunks 集合被裁剪的边界。
        let all_chunks = vec![chunk("c1", "Rust"), chunk("c2", "Rust 所有权")];
        let index = build(&all_chunks);

        let partial = vec![chunk("c2", "Rust 所有权")];
        let results = search(&index, &partial, "Rust", 5).expect("search ok");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "c2");
    }
}
