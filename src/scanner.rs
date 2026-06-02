//! 文件扫描模块 —— 递归遍历目录、过滤可处理文件、读取元数据。
//!
//! 负责人:谭张锐(见 `agent.md` §10、`plan.md` Day 2)。
//!
//! 设计要点(见 `docs/接口设计.md` §4):
//! - 仅支持 `.md` / `.txt`。
//! - 忽略隐藏文件和隐藏目录。
//! - 不在此模块读取文件正文,正文由 `parser` 模块负责。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

/// 文件元数据 —— 跨模块的稳定数据结构,字段见 `docs/接口设计.md` §2.1。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    /// 文件完整路径。
    pub path: PathBuf,
    /// 文件名(不含目录)。
    pub file_name: String,
    /// 文件大小(字节)。
    pub file_size: u64,
    /// 最后修改时间;读取失败时为 `None`。
    pub modified_time: Option<SystemTime>,
}

/// 递归扫描指定目录,返回所有支持的文件元数据。
///
/// 行为约定:
/// - `root` 不存在时返回 [`AppError::InvalidPath`](crate::error::AppError::InvalidPath)。
/// - 空目录返回空 `Vec`,不报错。
/// - 隐藏文件、`target/`、`.git/` 等需要被忽略。
///
/// TODO(谭张锐):用 `walkdir::WalkDir` 实现,并补单元测试
/// (见 `docs/测试计划.md` §3.1)。
pub fn scan_documents(_root: &Path) -> AppResult<Vec<DocumentMeta>> {
    todo!("递归扫描目录、过滤 .md/.txt、读取元数据 —— 见 docs/接口设计.md §4")
}
