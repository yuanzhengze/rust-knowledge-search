//! 索引缓存模块 —— 把 [`InvertedIndex`] 与 [`DocumentChunk`] 集合持久化到 JSON。
//!
//! 负责人:袁正泽(见 `agent.md` §10、`plan.md` Day 8)。
//!
//! 设计要点(见 `docs/接口设计.md` §9):
//! - 使用 [`serde_json`] 做格式,易读、便于调试。
//! - 缓存文件不存在 / JSON 损坏时返回 [`AppError::Index`](crate::error::AppError::Index)
//!   或 [`AppError::Serde`](crate::error::AppError::Serde),不要 panic。
//! - 默认缓存路径由 `KNOWLEDGE_INDEX_PATH` 环境变量决定,见 `.env.example`。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::chunker::DocumentChunk;
use crate::error::AppResult;
use crate::indexer::InvertedIndex;

/// 缓存文件的物理结构 —— `index` 命令写入,`search`/`ask` 命令读取。
///
/// 留作公共类型方便 storage 测试和后续工具直接反序列化(例如 dump 调试)。
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub index: InvertedIndex,
    pub chunks: Vec<DocumentChunk>,
}

/// 把索引和 chunk 集合保存到 `path` 指定的 JSON 文件。
///
/// TODO(袁正泽):实现 `serde_json::to_writer_pretty` + 原子写入
/// (见 `docs/测试计划.md` §3.6)。
pub fn save_index(
    _path: &Path,
    _index: &InvertedIndex,
    _chunks: &[DocumentChunk],
) -> AppResult<()> {
    todo!("把 InvertedIndex + chunks 序列化到 JSON —— 见 docs/接口设计.md §9")
}

/// 从 `path` 加载索引快照。
///
/// 行为约定:文件不存在或反序列化失败时返回 `Err`,调用方(`cli`)负责降级提示。
///
/// TODO(袁正泽):实现 `serde_json::from_reader` 并把错误映射成 `AppError`
/// (见 `docs/测试计划.md` §3.6)。
pub fn load_index(_path: &Path) -> AppResult<(InvertedIndex, Vec<DocumentChunk>)> {
    todo!("从 JSON 反序列化为 (InvertedIndex, Vec<DocumentChunk>) —— 见 docs/接口设计.md §9")
}
