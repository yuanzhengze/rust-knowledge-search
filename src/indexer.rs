//! 倒排索引模块 —— 把 [`DocumentChunk`] 集合编织为关键词到 chunk ID 的映射。
//!
//! 负责人:邱俊杰(见 `agent.md` §10、`plan.md` Day 5)。
//!
//! 设计要点(见 `docs/接口设计.md` §7):
//! - 至少记录关键词 → chunk ID 列表,以及词频(用于排序)。
//! - 中英文基础查询都需支持,后续可扩展 TF-IDF / BM25。
//! - 必须能 [`serde`] 序列化,以配合 [`crate::storage`] 缓存。
//!
//! 分词策略(零依赖,知识库场景够用):
//! - ASCII 字母数字累积成 token,统一小写化。
//! - CJK Unified Ideographs(U+4E00–U+9FFF)按字符 unigram 切分。
//! - 其他字符(标点、空白、其他脚本)作为分隔符。

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::chunker::DocumentChunk;

/// 倒排索引(字段公开,允许 [`crate::search`] 直接读取词频做评分)。
///
/// - `terms`:关键词 → 命中的 chunk ID 列表(单 chunk 内去重)。
/// - `term_freq`:关键词 → (chunk ID → 词频),用于 TF-IDF / BM25 排序。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InvertedIndex {
    pub terms: HashMap<String, Vec<String>>,
    pub term_freq: HashMap<String, HashMap<String, u32>>,
}

impl InvertedIndex {
    /// 根据 chunk 集合构建索引。
    ///
    /// - 同时分词 `title` 和 `content`,标题命中也算贡献。
    /// - `terms` 内每个 chunk 至多出现一次(便于布尔候选集运算)。
    /// - `term_freq` 累计每个 chunk 内每个 term 的真实出现次数。
    pub fn build(chunks: &[DocumentChunk]) -> Self {
        let mut index = InvertedIndex::default();

        for chunk in chunks {
            let mut tokens = tokenize(&chunk.content);
            if let Some(title) = &chunk.title {
                tokens.extend(tokenize(title));
            }
            if tokens.is_empty() {
                continue;
            }

            let mut seen: HashSet<&str> = HashSet::new();
            for token in &tokens {
                let count = index
                    .term_freq
                    .entry(token.clone())
                    .or_default()
                    .entry(chunk.id.clone())
                    .or_insert(0);
                *count += 1;

                if seen.insert(token.as_str()) {
                    index
                        .terms
                        .entry(token.clone())
                        .or_default()
                        .push(chunk.id.clone());
                }
            }
        }

        index
    }

    /// 查询关键词命中的 chunk ID 列表。
    ///
    /// - 不存在的关键词返回空 `Vec`。
    /// - 输入会做与 `build` 一致的规范化(去空白、英文转小写、CJK 取首字符)。
    pub fn lookup(&self, term: &str) -> Vec<String> {
        let normalized = match normalize_query_term(term) {
            Some(t) => t,
            None => return Vec::new(),
        };
        self.terms.get(&normalized).cloned().unwrap_or_default()
    }
}

/// 文本分词(中英文混合)。
///
/// 暴露给同 crate 的 [`crate::search`] 复用,保证索引侧和查询侧规则一致。
pub(crate) fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();

    for ch in text.chars() {
        if is_cjk(ch) {
            flush(&mut buf, &mut tokens);
            tokens.push(ch.to_string());
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            buf.push(ch.to_ascii_lowercase());
        } else {
            flush(&mut buf, &mut tokens);
        }
    }
    flush(&mut buf, &mut tokens);

    tokens
}

fn flush(buf: &mut String, tokens: &mut Vec<String>) {
    if !buf.is_empty() {
        tokens.push(std::mem::take(buf));
    }
}

/// 把 `lookup` 的单个查询词归一到 `tokenize` 能命中的形式。
///
/// 规则:
/// - 整体 trim 之后用 `tokenize` 拆一次,只保留第一个 token(`lookup` 单词语义)。
/// - 全空 / 全是分隔符时返回 `None`。
fn normalize_query_term(term: &str) -> Option<String> {
    tokenize(term.trim()).into_iter().next()
}

/// 判断字符是否落在 CJK Unified Ideographs 区间。
///
/// 仅覆盖最常用的中日韩汉字主区段(U+4E00–U+9FFF),避免引入更大的 Unicode 表。
/// 后续如果需要支持假名 / 韩文 / 扩展区,在此集中扩展即可。
fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x4E00..=0x9FFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    fn chunk(id: &str, content: &str, title: Option<&str>) -> DocumentChunk {
        DocumentChunk {
            id: id.to_string(),
            file_path: PathBuf::from(format!("/virtual/{id}.md")),
            title: title.map(|s| s.to_string()),
            content: content.to_string(),
            start_line: 1,
            end_line: 1,
        }
    }

    #[test]
    fn tokenize_should_split_english_and_cjk() {
        let tokens = tokenize("Rust 所有权 is GREAT! rust_lang");
        assert_eq!(
            tokens,
            vec![
                "rust".to_string(),
                "所".to_string(),
                "有".to_string(),
                "权".to_string(),
                "is".to_string(),
                "great".to_string(),
                "rust_lang".to_string(),
            ]
        );
    }

    #[test]
    fn tokenize_should_drop_punctuation_and_whitespace() {
        let tokens = tokenize(",,,  \n\t Rust!?。、 所有权 ");
        assert_eq!(
            tokens,
            vec![
                "rust".to_string(),
                "所".to_string(),
                "有".to_string(),
                "权".to_string()
            ]
        );
    }

    #[test]
    fn build_index_should_record_terms() {
        let chunks = vec![
            chunk("c1", "Rust 所有权 是核心概念", Some("Rust 所有权")),
            chunk("c2", "Rust 错误处理 Result", None),
        ];
        let index = InvertedIndex::build(&chunks);

        assert!(index.terms.contains_key("rust"));
        assert!(index.terms.contains_key("所"));
        let mut rust_chunks = index.terms.get("rust").cloned().unwrap();
        rust_chunks.sort();
        assert_eq!(rust_chunks, vec!["c1".to_string(), "c2".to_string()]);
    }

    #[test]
    fn lookup_should_return_matching_chunk_ids() {
        let chunks = vec![
            chunk("c1", "Rust 所有权 是核心概念", None),
            chunk("c2", "Rust 错误处理 Result", None),
            chunk("c3", "Python 装饰器", None),
        ];
        let index = InvertedIndex::build(&chunks);

        let mut hits = index.lookup("Rust");
        hits.sort();
        assert_eq!(hits, vec!["c1".to_string(), "c2".to_string()]);

        let cjk_hits = index.lookup("权");
        assert_eq!(cjk_hits, vec!["c1".to_string()]);
    }

    #[test]
    fn lookup_should_return_empty_for_unknown_term() {
        let index = InvertedIndex::build(&[chunk("c1", "Rust 基础", None)]);
        assert!(index.lookup("kotlin").is_empty());
        assert!(index.lookup("龘").is_empty());
        assert!(index.lookup("").is_empty());
        assert!(index.lookup("   ").is_empty());
        assert!(index.lookup(",,,").is_empty());
    }

    #[test]
    fn build_index_should_record_term_freq() {
        let chunks = vec![chunk("c1", "Rust Rust rust 所有 所有权", None)];
        let index = InvertedIndex::build(&chunks);

        let rust_freq = index
            .term_freq
            .get("rust")
            .and_then(|m| m.get("c1").copied())
            .unwrap_or(0);
        assert_eq!(rust_freq, 3, "Rust 大小写都应该归到同一个 term");

        let suo_freq = index
            .term_freq
            .get("所")
            .and_then(|m| m.get("c1").copied())
            .unwrap_or(0);
        assert_eq!(suo_freq, 2);

        // terms 列表内单 chunk 不重复
        let rust_chunks = index.terms.get("rust").cloned().unwrap_or_default();
        assert_eq!(rust_chunks, vec!["c1".to_string()]);
    }

    #[test]
    fn build_index_should_include_title_tokens() {
        let chunks = vec![chunk("c1", "正文不含关键字", Some("Rust 所有权"))];
        let index = InvertedIndex::build(&chunks);

        assert_eq!(index.lookup("Rust"), vec!["c1".to_string()]);
        assert_eq!(index.lookup("权"), vec!["c1".to_string()]);
    }

    #[test]
    fn build_index_should_skip_empty_chunks() {
        let chunks = vec![chunk("c1", "   \n\t,。", None), chunk("c2", "Rust", None)];
        let index = InvertedIndex::build(&chunks);
        assert!(!index.terms.contains_key("c1"));
        assert_eq!(index.lookup("Rust"), vec!["c2".to_string()]);
    }

    #[test]
    fn index_should_round_trip_through_serde() {
        let chunks = vec![chunk("c1", "Rust 所有权", None)];
        let index = InvertedIndex::build(&chunks);

        let json = serde_json::to_string(&index).expect("serialize index");
        let restored: InvertedIndex = serde_json::from_str(&json).expect("deserialize index");
        assert_eq!(restored.lookup("Rust"), vec!["c1".to_string()]);
        assert_eq!(restored.lookup("权"), vec!["c1".to_string()]);
    }
}
