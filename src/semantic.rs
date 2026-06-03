//! 语义增强模块 —— 可选 AI 排序与问答能力。
//!
//! 负责人:谭张锐(Day 10–11 任务移交,见 `plan.md` §5、`agent.md` §10)。
//! 原 scanner 工作已收尾,谭张锐接手语义增强模块。
//!
//! 设计要点(见 `docs/接口设计.md` §10):
//! - 接入方式可换:外部 embedding API、本地模型、规则化降级。
//! - API Key / Base URL 等从环境变量读取(`AI_API_KEY` / `AI_BASE_URL` /
//!   `AI_CHAT_MODEL`,见 `.env.example`),严禁硬编码。
//! - 网络 / API 失败必须返回 [`AppError::Semantic`](crate::error::AppError::Semantic),
//!   `cli` 的 `ask` 命令会捕获并降级为关键词检索结果。
//!
//! ## 当前实现
//!
//! - [`NoopEngine`]:占位实现,所有方法返回 `AppError::Semantic("disabled")`,
//!   未配置 AI 时由 `cli` 选用。
//! - [`ChatEngine`]:调用 **OpenAI 兼容的 Chat Completions API**(DeepSeek、
//!   智谱、月之暗面、SiliconFlow、OpenRouter、OpenAI 等都支持同一协议)。
//!   通过 [`ChatEngine::from_env`] 在配置 `AI_API_KEY` 时启用。

use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::chunker::DocumentChunk;
use crate::error::{AppError, AppResult};

/// 语义引擎不可用时统一吐出的错误信息。
/// CLI 的 `ask` 命令依赖这个串作为"降级到候选片段展示"的明确信号。
const DISABLED_MSG: &str = "semantic engine disabled";

/// `ChatEngine` 默认 base URL —— 没设 `AI_BASE_URL` 时回落到 OpenAI 官方端点。
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
/// 默认 chat 模型 —— 没设 `AI_CHAT_MODEL` 时使用。
const DEFAULT_CHAT_MODEL: &str = "gpt-4o-mini";
/// HTTP 请求超时,避免 ask 命令在网络异常时长时间挂起。
const HTTP_TIMEOUT_SECS: u64 = 30;
/// 提交给 LLM 的 chunk 内容截断长度(按 char,UTF-8 安全),防止 prompt 过长。
const MAX_CONTEXT_CHARS_PER_CHUNK: usize = 600;

/// 语义增强引擎 —— 提供向量重排和问答两种能力。
///
/// 实现方需保证:任何方法的失败路径都通过 `AppError::Semantic` 表达,而不是 panic。
pub trait SemanticEngine {
    /// 对候选 chunk 做语义重排,返回 `(chunk_id, score)` 列表(越大越相关)。
    fn rank(&self, query: &str, chunks: &[DocumentChunk]) -> AppResult<Vec<(String, f64)>>;

    /// 基于上下文 chunk 给出对自然语言问题的简短回答。
    fn answer(&self, question: &str, contexts: &[DocumentChunk]) -> AppResult<String>;
}

// ---------------------------------------------------------------------------
// NoopEngine —— AI 不可用时的占位实现
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// ChatEngine —— 调用 OpenAI 兼容的 Chat Completions API
// ---------------------------------------------------------------------------

/// 从环境变量解析得到的 chat 引擎配置。
///
/// 公开是为了让 [`ChatEngine::new`] 与单测可以独立构造,不依赖全局 env。
#[derive(Debug, Clone)]
pub struct ChatConfig {
    /// `AI_API_KEY`,必填。
    pub api_key: String,
    /// `AI_BASE_URL`,可选,默认 OpenAI 官方端点。
    pub base_url: String,
    /// `AI_CHAT_MODEL`,可选,默认 `gpt-4o-mini`。
    pub chat_model: String,
}

/// 从外部 getter 构造 [`ChatConfig`]。getter 通常是 `|k| std::env::var(k).ok()`,
/// 但单测可以传任意 closure,避免污染全局环境变量状态。
///
/// 缺失或为空字符串的 `AI_API_KEY` 视为未配置,返回 `None`,让 cli 走 NoopEngine。
pub fn chat_config_from_env<F>(getter: F) -> Option<ChatConfig>
where
    F: Fn(&str) -> Option<String>,
{
    let api_key = getter("AI_API_KEY").filter(|s| !s.is_empty())?;
    let base_url = getter("AI_BASE_URL")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let chat_model = getter("AI_CHAT_MODEL")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_CHAT_MODEL.to_string());
    Some(ChatConfig {
        api_key,
        base_url,
        chat_model,
    })
}

/// 调用 OpenAI 兼容的 Chat Completions API 给 `ask` 命令提供真实 AI 回答。
///
/// 兼容协议:DeepSeek、智谱(zhipu)、月之暗面(Kimi)、SiliconFlow、OpenRouter、
/// OpenAI 等都暴露 `POST {base_url}/chat/completions` 端点,请求/响应字段
/// 在 chat 子集上一致,所以一份代码可以接入所有这些后端。
pub struct ChatEngine {
    config: ChatConfig,
    client: Client,
}

impl ChatEngine {
    /// 用显式配置构造 `ChatEngine`。HTTP client 失败时返回 `AppError::Semantic`。
    pub fn new(config: ChatConfig) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .build()
            .map_err(|e| AppError::Semantic(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { config, client })
    }

    /// 从环境变量构造 `ChatEngine`。`AI_API_KEY` 未配置或 client 构造失败时返回 `None`。
    /// `cli` 拿到 `None` 就回落到 `NoopEngine`,符合 `agent.md` §1 的"AI 可降级"原则。
    pub fn from_env() -> Option<Self> {
        let config = chat_config_from_env(|k| std::env::var(k).ok())?;
        Self::new(config).ok()
    }
}

impl SemanticEngine for ChatEngine {
    /// 当前版本的 `rank` 不做语义重排(等价于"不调整顺序")。
    /// 这是合理的占位:`search` 已经返回了基于词频排序的结果,直接复用。
    /// 后续如果接入真正的 embedding API,可以在这里做向量相似度重排。
    fn rank(&self, _query: &str, chunks: &[DocumentChunk]) -> AppResult<Vec<(String, f64)>> {
        Ok(chunks
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.clone(), (chunks.len() - i) as f64))
            .collect())
    }

    fn answer(&self, question: &str, contexts: &[DocumentChunk]) -> AppResult<String> {
        let context_text = build_context_prompt(contexts);

        let body = ChatRequest {
            model: self.config.chat_model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: format!(
                        "你是一个本地知识库助手。基于下面提供的文档片段回答用户问题,\
                         直接给出结论,引用来源用 [片段编号] 标注。如果片段中没有相关信息,\
                         请明确说明而不是编造。回答用中文,不超过 200 字。\n\n\
                         文档片段:\n{context_text}"
                    ),
                },
                ChatMessage {
                    role: "user".into(),
                    content: question.to_string(),
                },
            ],
            temperature: 0.2,
        };

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .map_err(|e| AppError::Semantic(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().unwrap_or_default();
            // 截断响应体,避免把整个 HTML 错误页面塞进 AppError 里。
            let snippet: String = body_text.chars().take(200).collect();
            return Err(AppError::Semantic(format!(
                "AI API returned {status}: {snippet}"
            )));
        }

        let chat_resp: ChatResponse = response
            .json()
            .map_err(|e| AppError::Semantic(format!("invalid JSON response: {e}")))?;

        chat_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| AppError::Semantic("AI response had no usable content".into()))
    }
}

/// 把候选 chunks 拼成 LLM 的 system prompt 上下文。
/// 每个 chunk 加 `[1] / [2] / ...` 编号,便于模型在回答里引用,
/// 并按 [`MAX_CONTEXT_CHARS_PER_CHUNK`] 截断,防止 prompt 过长。
fn build_context_prompt(chunks: &[DocumentChunk]) -> String {
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let truncated: String = c
                .content
                .chars()
                .take(MAX_CONTEXT_CHARS_PER_CHUNK)
                .collect();
            let suffix = if c.content.chars().count() > MAX_CONTEXT_CHARS_PER_CHUNK {
                "…"
            } else {
                ""
            };
            format!(
                "[{}] {} (L{}-L{})\n{}{}",
                i + 1,
                c.id,
                c.start_line,
                c.end_line,
                truncated,
                suffix
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ---------------------------------------------------------------------------
// OpenAI 兼容协议的请求/响应数据结构
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize, Debug)]
struct ChatChoice {
    message: ChatMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::path::PathBuf;

    fn sample_chunk(id: &str, content: &str) -> DocumentChunk {
        DocumentChunk {
            id: id.to_string(),
            file_path: PathBuf::from(format!("/v/{id}.md")),
            title: None,
            content: content.to_string(),
            start_line: 1,
            end_line: 5,
        }
    }

    /// 用 HashMap 模拟环境变量,完全不动 std::env 全局状态,测试可以并行跑。
    fn env_getter(map: HashMap<&'static str, &'static str>) -> impl Fn(&str) -> Option<String> {
        move |k: &str| map.get(k).map(|v| v.to_string())
    }

    // ---------------- NoopEngine ----------------

    #[test]
    fn noop_engine_rank_should_return_semantic_error() {
        let engine = NoopEngine;
        let chunks = vec![sample_chunk("c1", "Rust")];
        let err = engine.rank("Rust", &chunks).expect_err("rank must error");
        match err {
            AppError::Semantic(msg) => assert!(msg.contains("disabled")),
            other => panic!("expected Semantic, got {other:?}"),
        }
    }

    #[test]
    fn noop_engine_answer_should_return_semantic_error() {
        let engine = NoopEngine;
        let chunks = vec![sample_chunk("c1", "Rust")];
        let err = engine
            .answer("什么是所有权?", &chunks)
            .expect_err("answer must error");
        match err {
            AppError::Semantic(msg) => assert!(msg.contains("disabled")),
            other => panic!("expected Semantic, got {other:?}"),
        }
    }

    // ---------------- chat_config_from_env ----------------

    #[test]
    fn chat_config_should_be_none_without_api_key() {
        let cfg = chat_config_from_env(env_getter(HashMap::new()));
        assert!(cfg.is_none(), "缺 AI_API_KEY 时应返回 None");
    }

    #[test]
    fn chat_config_should_be_none_for_empty_api_key() {
        let mut m = HashMap::new();
        m.insert("AI_API_KEY", "");
        let cfg = chat_config_from_env(env_getter(m));
        assert!(cfg.is_none(), "空字符串 AI_API_KEY 也应视为未配置");
    }

    #[test]
    fn chat_config_should_use_defaults_for_optional_fields() {
        let mut m = HashMap::new();
        m.insert("AI_API_KEY", "sk-test");
        let cfg = chat_config_from_env(env_getter(m)).expect("should be Some");
        assert_eq!(cfg.api_key, "sk-test");
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
        assert_eq!(cfg.chat_model, DEFAULT_CHAT_MODEL);
    }

    #[test]
    fn chat_config_should_honor_custom_base_url_and_model() {
        let mut m = HashMap::new();
        m.insert("AI_API_KEY", "sk-test");
        m.insert("AI_BASE_URL", "https://api.deepseek.com/v1");
        m.insert("AI_CHAT_MODEL", "deepseek-chat");
        let cfg = chat_config_from_env(env_getter(m)).expect("should be Some");
        assert_eq!(cfg.base_url, "https://api.deepseek.com/v1");
        assert_eq!(cfg.chat_model, "deepseek-chat");
    }

    #[test]
    fn chat_config_empty_optional_should_fall_back_to_default() {
        let mut m = HashMap::new();
        m.insert("AI_API_KEY", "sk-test");
        m.insert("AI_BASE_URL", "");
        m.insert("AI_CHAT_MODEL", "");
        let cfg = chat_config_from_env(env_getter(m)).expect("should be Some");
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
        assert_eq!(cfg.chat_model, DEFAULT_CHAT_MODEL);
    }

    // ---------------- ChatEngine 构造 ----------------

    #[test]
    fn chat_engine_new_should_succeed_with_valid_config() {
        let cfg = ChatConfig {
            api_key: "sk-test".into(),
            base_url: DEFAULT_BASE_URL.into(),
            chat_model: DEFAULT_CHAT_MODEL.into(),
        };
        let engine = ChatEngine::new(cfg);
        assert!(engine.is_ok(), "正常配置下 client 构造应当成功");
    }

    #[test]
    fn chat_engine_rank_should_preserve_order() {
        // ChatEngine.rank 当前是占位实现,按输入顺序给降序分数,不真调网络。
        let cfg = ChatConfig {
            api_key: "sk-test".into(),
            base_url: DEFAULT_BASE_URL.into(),
            chat_model: DEFAULT_CHAT_MODEL.into(),
        };
        let engine = ChatEngine::new(cfg).expect("build engine");
        let chunks = vec![
            sample_chunk("c1", "first"),
            sample_chunk("c2", "second"),
            sample_chunk("c3", "third"),
        ];
        let ranked = engine.rank("query", &chunks).expect("rank ok");
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].0, "c1");
        // 分数应当严格递减
        assert!(ranked[0].1 > ranked[1].1);
        assert!(ranked[1].1 > ranked[2].1);
    }

    // ---------------- build_context_prompt ----------------

    #[test]
    fn build_context_prompt_should_include_index_and_id() {
        let chunks = vec![
            sample_chunk("c1", "Rust 所有权"),
            sample_chunk("c2", "错误处理"),
        ];
        let prompt = build_context_prompt(&chunks);
        assert!(prompt.contains("[1]"));
        assert!(prompt.contains("[2]"));
        assert!(prompt.contains("c1"));
        assert!(prompt.contains("c2"));
        assert!(prompt.contains("Rust 所有权"));
    }

    #[test]
    fn build_context_prompt_should_truncate_long_chunks() {
        let long_content = "a".repeat(MAX_CONTEXT_CHARS_PER_CHUNK + 100);
        let chunks = vec![sample_chunk("c1", &long_content)];
        let prompt = build_context_prompt(&chunks);
        assert!(prompt.contains("…"), "超长 chunk 应附 …");
        // prompt 长度应受约束
        let chars = prompt.chars().count();
        assert!(
            chars < long_content.chars().count() + 200,
            "prompt 总长 {chars} 应远短于原 chunk"
        );
    }
}
