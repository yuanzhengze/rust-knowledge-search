//! 倒排索引模块 —— 把 [`DocumentChunk`] 集合编织为关键词到 chunk ID 的映射。
//!
//! 负责人:邱俊杰(见 `agent.md` §10、`plan.md` Day 5)。
//!
//! 设计要点(见 `docs/接口设计.md` §7):
//! - 至少记录关键词 → chunk ID 列表,以及词频(用于排序)。
//! - 中英文基础查询都需支持,后续可扩展 TF-IDF / BM25。
//! - 必须能 [`serde`] 序列化,以配合 [`crate::storage`] 缓存。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::chunker::DocumentChunk;

/// 倒排索引(字段为占位实现,允许后续在 `feature/indexer` 分支调整)。
///
/// - `terms`:关键词 → 命中的 chunk ID 列表。
/// - `term_freq`:关键词 → (chunk ID → 词频),用于 TF-IDF / BM25 排序。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InvertedIndex {
    pub terms: HashMap<String, Vec<String>>,
    pub term_freq: HashMap<String, HashMap<String, u32>>,
}

impl InvertedIndex {
    /// 根据 chunk 集合构建索引。
    ///
    /// TODO(邱俊杰):实现分词 + 倒排表构建
    /// (见 `docs/测试计划.md` §3.4)。
    pub fn build(_chunks: &[DocumentChunk]) -> Self {
        todo!("分词、构建倒排表、记录词频 —— 见 docs/接口设计.md §7")
    }

    /// 查询关键词命中的 chunk ID 列表。
    ///
    /// - 不存在的关键词返回空 `Vec`,不应返回 `Err`。
    /// - 关键词需要做与 `build` 一致的规范化(大小写、Unicode normalization)。
    pub fn lookup(&self, _term: &str) -> Vec<String> {
        todo!("根据规范化后的关键词从 terms 中取候选 chunk —— 见 docs/接口设计.md §7")
    }
}
