//! 基于 Rust 的本地知识库语义搜索系统 —— 库入口。
//!
//! 模块职责见 `agent.md` §4 和 `docs/接口设计.md`。
//! 所有公共类型和错误统一在此 re-export,方便集成测试和命令行入口引用。

pub mod chunker;
pub mod cli;
pub mod error;
pub mod indexer;
pub mod parser;
pub mod scanner;
pub mod search;
pub mod semantic;
pub mod storage;

pub use chunker::DocumentChunk;
pub use error::{AppError, AppResult};
pub use indexer::InvertedIndex;
pub use parser::Document;
pub use scanner::DocumentMeta;
pub use search::SearchResult;
pub use semantic::{NoopEngine, SemanticEngine};
