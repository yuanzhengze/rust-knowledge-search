//! 文档解析模块 —— 读取文件、清洗 Markdown/TXT、提取标题。
//!
//! 负责人:陈文涛(见 `agent.md` §10、`plan.md` Day 3)。
//!
//! 设计要点(见 `docs/接口设计.md` §5):
//! - Markdown 去除基础标记,保留正文。
//! - TXT 直接读取。
//! - 非 UTF-8 时返回 [`AppError::Parse`](crate::error::AppError::Parse)。

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::scanner::DocumentMeta;

/// 解析后的完整文档 —— 见 `docs/接口设计.md` §2.2。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// 文档元数据。
    pub meta: DocumentMeta,
    /// 文档标题:Markdown 取首个一级标题,TXT 取首个非空行或 `None`。
    pub title: Option<String>,
    /// 清洗后的正文。
    pub content: String,
}

/// 读取并解析单个文档。
///
/// 行为约定:
/// - 后缀为 `.md` 时,去除 Markdown 标记;`.txt` 直接读取。
/// - 文件读取失败 / 非 UTF-8 时返回 `Err`,不要 `unwrap`。
///
/// TODO(陈文涛):实现 Markdown 标题提取与正文清洗
/// (见 `docs/测试计划.md` §3.2)。
pub fn parse_document(_meta: DocumentMeta) -> AppResult<Document> {
    todo!("读取文件 + 清洗 Markdown/TXT + 提取标题 —— 见 docs/接口设计.md §5")
}
