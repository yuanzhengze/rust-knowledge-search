//! 语义增强模块 —— 可选 AI 排序与问答能力。
//!
//! 负责人:袁正泽(见 `agent.md` §10、`plan.md` Day 10–11)。
//!
//! 设计要点(见 `docs/接口设计.md` §10):
//! - 接入方式可换:外部 embedding API、本地模型、规则化降级。
//! - API Key / Base URL 等从环境变量读取(`AI_API_KEY` 等,见 `.env.example`),
//!   严禁硬编码。
//! - 网络 / API 失败必须返回 [`AppError::Semantic`](crate::error::AppError::Semantic),
//!   `cli` 的 `ask` 命令会捕获并降级为关键词检索结果。

use crate::chunker::DocumentChunk;
use crate::error::{AppError, AppResult};

/// 语义增强引擎 —— 提供向量重排和问答两种能力。
///
/// 实现方需保证:任何方法的失败路径都通过 `AppError::Semantic` 表达,而不是 panic。
pub trait SemanticEngine {
    /// 对候选 chunk 做语义重排,返回 `(chunk_id, score)` 列表(越大越相关)。
    fn rank(&self, query: &str, chunks: &[DocumentChunk]) -> AppResult<Vec<(String, f64)>>;

    /// 基于上下文 chunk 给出对自然语言问题的简短回答。
    fn answer(&self, question: &str, contexts: &[DocumentChunk]) -> AppResult<String>;
}

/// 语义引擎不可用时统一吐出的错误信息。
/// CLI 的 `ask` 命令依赖这个串作为"降级到候选片段展示"的明确信号。
const DISABLED_MSG: &str = "semantic engine disabled";

/// 占位实现 —— 当 AI 不可用(没配 API Key、断网、依赖缺失)时,
/// `cli::run` 会用它作为 fallback,所有方法返回 `AppError::Semantic`,
/// 由 CLI 层捕获后降级为关键词检索结果展示。
///
/// 这一占位的存在让 `agent.md` §1 的"AI 可降级"原则在类型系统里可见。
pub struct NoopEngine;

impl SemanticEngine for NoopEngine {
    fn rank(&self, _query: &str, _chunks: &[DocumentChunk]) -> AppResult<Vec<(String, f64)>> {
        Err(AppError::Semantic(DISABLED_MSG.into()))
    }

    fn answer(&self, _question: &str, _contexts: &[DocumentChunk]) -> AppResult<String> {
        Err(AppError::Semantic(DISABLED_MSG.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    fn sample_chunk() -> DocumentChunk {
        DocumentChunk {
            id: "c1".into(),
            file_path: PathBuf::from("/v/c1.md"),
            title: None,
            content: "Rust".into(),
            start_line: 1,
            end_line: 1,
        }
    }

    #[test]
    fn noop_engine_rank_should_return_semantic_error() {
        let engine = NoopEngine;
        let chunks = vec![sample_chunk()];
        let err = engine.rank("Rust", &chunks).expect_err("rank must error");
        match err {
            AppError::Semantic(msg) => assert!(msg.contains("disabled")),
            other => panic!("expected Semantic, got {other:?}"),
        }
    }

    #[test]
    fn noop_engine_answer_should_return_semantic_error() {
        let engine = NoopEngine;
        let chunks = vec![sample_chunk()];
        let err = engine
            .answer("什么是所有权?", &chunks)
            .expect_err("answer must error");
        match err {
            AppError::Semantic(msg) => assert!(msg.contains("disabled")),
            other => panic!("expected Semantic, got {other:?}"),
        }
    }
}
