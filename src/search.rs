//! 搜索排序模块 —— 把查询字符串变成排序后的 [`SearchResult`] 列表。
//!
//! 负责人:邱俊杰(见 `agent.md` §10、`plan.md` Day 6)。
//!
//! 设计要点(见 `docs/接口设计.md` §8):
//! - 空查询返回 [`AppError::Index`](crate::error::AppError::Index)。
//! - 无结果时返回空 `Vec`,不应崩溃。
//! - 排序逻辑需要可单测(纯函数 + 输入输出可控)。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::chunker::DocumentChunk;
use crate::error::AppResult;
use crate::indexer::InvertedIndex;

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

/// 在索引上执行查询并返回 Top K 结果。
///
/// 行为约定:
/// - 空 / 仅含空白的查询返回 `Err(AppError::Index(_))`。
/// - 候选 chunk 通过 [`InvertedIndex::lookup`] 取出后再做评分排序。
/// - `top_k == 0` 视为非法输入,返回 `Err`。
///
/// TODO(邱俊杰):实现词频或 TF-IDF 排序
/// (见 `docs/测试计划.md` §3.5)。
pub fn search(
    _index: &InvertedIndex,
    _chunks: &[DocumentChunk],
    _query: &str,
    _top_k: usize,
) -> AppResult<Vec<SearchResult>> {
    todo!("查询关键词 + 评分排序 + 取 Top K —— 见 docs/接口设计.md §8")
}
