//! 文档切片模块 —— 把 [`Document`] 拆分为多个 [`DocumentChunk`]。
//!
//! 负责人:陈文涛(见 `agent.md` §10、`plan.md` Day 4)。
//!
//! 设计要点(见 `docs/接口设计.md` §6):
//! - chunk 大小由 `max_chars` 控制,过短的文档至少返回一个 chunk。
//! - chunk ID 必须稳定且唯一,推荐 `<file_path>#<start_line>-<end_line>` 形式。
//! - 必须保留来源文件路径与行号范围,方便搜索结果回溯原文。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::parser::Document;

/// 搜索 / 语义分析的最小文本片段 —— 见 `docs/接口设计.md` §2.3。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    /// 片段唯一 ID(在整个索引内不重复)。
    pub id: String,
    /// 来源文件路径。
    pub file_path: PathBuf,
    /// 来源文档标题。
    pub title: Option<String>,
    /// 片段文本内容。
    pub content: String,
    /// 起始行号(0-based 或 1-based 由实现决定,需在文档中声明)。
    pub start_line: usize,
    /// 结束行号(含)。
    pub end_line: usize,
}

/// 把单个文档切分为若干 chunk。
///
/// 行为约定:
/// - 长度 `<= max_chars` 的文档应至少返回 1 个 chunk。
/// - chunk 之间允许少量重叠以提升语义连续性,具体策略由实现决定。
///
/// TODO(陈文涛):按行 / 段落 / 字符切分并生成稳定 ID
/// (见 `docs/测试计划.md` §3.3)。
pub fn chunk_document(_document: &Document, _max_chars: usize) -> Vec<DocumentChunk> {
    todo!("把 Document 切分为 DocumentChunk —— 见 docs/接口设计.md §6")
}
