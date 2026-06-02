//! 项目统一错误类型。
//!
//! 所有可失败函数返回 [`AppResult<T>`],变体覆盖 IO、路径、解析、索引和语义错误。
//! 见 `docs/接口设计.md` §3。
//!
//! 这是骨架阶段唯一**不**使用 `todo!()` 的业务模块 —— 其它模块的占位签名都依赖
//! `AppResult<T>` 才能编译通过。

use thiserror::Error;

/// 项目统一错误类型。
///
/// 新增变体时需同步更新 `docs/接口设计.md` §3,并通知相关模块负责人。
#[derive(Debug, Error)]
pub enum AppError {
    /// 透传标准 IO 错误,文件读写、目录扫描等场景使用。
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 路径不存在、不可达、不是目录等。
    #[error("invalid path: {0}")]
    InvalidPath(String),

    /// 文档解析失败,如非 UTF-8、Markdown 格式异常。
    #[error("parse error: {0}")]
    Parse(String),

    /// 索引构建或查询过程出错,如缓存损坏、关键词分词失败。
    #[error("index error: {0}")]
    Index(String),

    /// 语义增强模块出错,如 API Key 缺失、网络失败、模型返回异常。
    #[error("semantic error: {0}")]
    Semantic(String),

    /// JSON 序列化 / 反序列化错误,storage 模块用。
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// 项目统一返回类型,所有可失败函数都用它。
pub type AppResult<T> = Result<T, AppError>;
