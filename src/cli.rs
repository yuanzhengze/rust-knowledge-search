//! 命令行入口 —— 用 [`clap`] 的 derive 模式定义子命令并分发。
//!
//! 负责人:黄开轩(见 `agent.md` §10、`plan.md` Day 7)。
//!
//! 命令布局(见 `docs/接口设计.md` §11):
//!
//! ```text
//! rust-knowledge-search index <path>
//! rust-knowledge-search search <query> [--top N]
//! rust-knowledge-search ask <question>
//! ```
//!
//! 设计要点:
//! - 真正的业务逻辑放在 [`run_index_at`] / [`run_search_at`] / [`run_ask_at`]
//!   三个 `*_at` 函数里,显式接收 `cache_path` 参数;[`run`] 只做 clap 解析和
//!   环境变量解析。这样集成测试可以传临时缓存路径,不污染当前目录。
//! - 缓存路径优先级:`KNOWLEDGE_INDEX_PATH` 环境变量 > 默认 `.knowledge_index.json`。
//!   `.knowledge_index.json` 已经在 `.gitignore` 中,不会被误提交。
//! - `ask` 命令在 [`SemanticEngine::answer`] 返回 [`AppError::Semantic`] 时降级
//!   展示候选片段(`agent.md` §1 的"AI 可降级"原则)。

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::chunker::{chunk_document, DocumentChunk};
use crate::error::{AppError, AppResult};
use crate::indexer::InvertedIndex;
use crate::parser::parse_document;
use crate::scanner::scan_documents;
use crate::search::search;
use crate::semantic::{NoopEngine, SemanticEngine};
use crate::storage::{load_index, save_index};

/// 默认缓存文件名(相对当前工作目录),与 `.gitignore` 保持一致。
const DEFAULT_CACHE: &str = ".knowledge_index.json";
/// 缓存路径环境变量名,见 `.env.example`。
const CACHE_ENV: &str = "KNOWLEDGE_INDEX_PATH";
/// 文档切片的字符上限,与 `chunker` 单测里使用的值保持一致。
const DEFAULT_CHUNK_CHARS: usize = 200;
/// `ask` 命令默认拉取的候选片段数。
const ASK_TOP_K: usize = 5;

/// 顶层 CLI 解析器。
#[derive(Debug, Parser)]
#[command(
    name = "rust-knowledge-search",
    version,
    about = "基于 Rust 的本地知识库语义搜索系统",
    long_about = "扫描本地 Markdown/TXT 文档,构建关键词索引,支持关键词搜索与可选的自然语言问答。"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// 三个核心子命令。
#[derive(Debug, Subcommand)]
pub enum Command {
    /// 扫描指定目录并构建索引缓存。
    Index {
        /// 知识库根目录(包含 .md / .txt 文件)。
        path: PathBuf,
    },
    /// 用关键词在已建索引中搜索。
    Search {
        /// 查询字符串(中英文均可)。
        query: String,
        /// 返回结果数量上限。
        #[arg(long, default_value_t = 5)]
        top: usize,
    },
    /// 用自然语言问题查询,可选调用 AI 总结。
    Ask {
        /// 自然语言问题。
        question: String,
    },
}

/// 程序主流程 —— 解析参数后分发到具体命令实现。
pub fn run() -> AppResult<()> {
    let cli = Cli::parse();
    let cache = resolve_cache_path();
    match cli.command {
        Command::Index { path } => run_index_at(&path, &cache),
        Command::Search { query, top } => run_search_at(&cache, &query, top),
        Command::Ask { question } => run_ask_at(&cache, &question),
    }
}

/// 根据 `KNOWLEDGE_INDEX_PATH` 环境变量解析缓存路径,缺省回退到 [`DEFAULT_CACHE`]。
pub fn resolve_cache_path() -> PathBuf {
    cache_path_from_env(std::env::var(CACHE_ENV).ok().as_deref())
}

/// 抽出来的纯函数版本,便于单测覆盖而不依赖全局环境变量状态。
fn cache_path_from_env(env_value: Option<&str>) -> PathBuf {
    env_value
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CACHE))
}

/// `index` 子命令实现:扫描目录 → 解析 → 切片 → 构建索引 → 写入缓存。
pub fn run_index_at(root: &Path, cache: &Path) -> AppResult<()> {
    let metas = scan_documents(root)?;
    println!("扫描到 {} 个文档", metas.len());

    let mut all_chunks = Vec::new();
    for meta in metas {
        let doc = parse_document(meta)?;
        all_chunks.extend(chunk_document(&doc, DEFAULT_CHUNK_CHARS));
    }
    println!("切分得到 {} 个片段", all_chunks.len());

    let index = InvertedIndex::build(&all_chunks);
    println!("索引共 {} 个关键词", index.terms.len());

    save_index(cache, &index, &all_chunks)?;
    println!("索引已保存到 {}", cache.display());
    Ok(())
}

/// `search` 子命令实现:加载缓存 → 检索 → 渲染。
pub fn run_search_at(cache: &Path, query: &str, top: usize) -> AppResult<()> {
    let (index, chunks) = load_index(cache)?;
    let results = search(&index, &chunks, query, top)?;

    if results.is_empty() {
        println!("未找到与「{query}」相关的文档片段。");
        return Ok(());
    }

    println!("找到 {} 条结果:\n", results.len());
    for (i, r) in results.iter().enumerate() {
        println!("[{}] {}  分数 {:.2}", i + 1, r.chunk_id, r.score);
        if !r.matched_terms.is_empty() {
            println!("    匹配关键词: {}", r.matched_terms.join(", "));
        }
        println!("    {}", r.snippet);
        println!();
    }
    Ok(())
}

/// `ask` 子命令实现:加载缓存 → 检索候选 → 调用 [`SemanticEngine::answer`],
/// 失败时降级展示候选片段。
pub fn run_ask_at(cache: &Path, question: &str) -> AppResult<()> {
    let (index, chunks) = load_index(cache)?;
    let results = search(&index, &chunks, question, ASK_TOP_K)?;

    if results.is_empty() {
        println!("未找到与该问题相关的文档片段。");
        return Ok(());
    }

    // 取出命中片段对应的 DocumentChunk 作为语义引擎的上下文。
    // top_k 通常 <= 10,这里 O(n*m) 足够,后续如需优化可换 HashMap。
    let context_chunks: Vec<DocumentChunk> = chunks
        .iter()
        .filter(|c| results.iter().any(|r| r.chunk_id == c.id))
        .cloned()
        .collect();

    let engine = NoopEngine;
    match engine.answer(question, &context_chunks) {
        Ok(answer) => {
            println!("AI 回答:\n{answer}\n");
            println!("引用片段:\n");
        }
        Err(AppError::Semantic(msg)) => {
            println!("[提示] AI 引擎不可用 ({msg})。以下是检索到的相关片段:\n");
        }
        Err(other) => return Err(other),
    }

    for (i, r) in results.iter().enumerate() {
        println!("[{}] {}  分数 {:.2}", i + 1, r.chunk_id, r.score);
        println!("    {}", r.snippet);
        println!();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_from_env_should_use_value_when_present() {
        let path = cache_path_from_env(Some("/tmp/custom.json"));
        assert_eq!(path, PathBuf::from("/tmp/custom.json"));
    }

    #[test]
    fn cache_path_from_env_should_default_when_missing() {
        let path = cache_path_from_env(None);
        assert_eq!(path, PathBuf::from(DEFAULT_CACHE));
    }

    #[test]
    fn cache_path_from_env_should_default_when_empty() {
        // 空字符串视为未设置,避免误把 "" 当成根目录下的文件。
        let path = cache_path_from_env(Some(""));
        assert_eq!(path, PathBuf::from(DEFAULT_CACHE));
    }

    #[test]
    fn cli_parses_index_subcommand() {
        let cli = Cli::try_parse_from(["rust-knowledge-search", "index", "./notes"]).unwrap();
        match cli.command {
            Command::Index { path } => assert_eq!(path, PathBuf::from("./notes")),
            other => panic!("expected Index, got {other:?}"),
        }
    }

    #[test]
    fn cli_parses_search_with_top() {
        let cli =
            Cli::try_parse_from(["rust-knowledge-search", "search", "Rust", "--top", "3"]).unwrap();
        match cli.command {
            Command::Search { query, top } => {
                assert_eq!(query, "Rust");
                assert_eq!(top, 3);
            }
            other => panic!("expected Search, got {other:?}"),
        }
    }

    #[test]
    fn cli_search_top_defaults_to_five() {
        let cli = Cli::try_parse_from(["rust-knowledge-search", "search", "Rust"]).unwrap();
        match cli.command {
            Command::Search { top, .. } => assert_eq!(top, 5),
            other => panic!("expected Search, got {other:?}"),
        }
    }

    #[test]
    fn cli_parses_ask_subcommand() {
        let cli =
            Cli::try_parse_from(["rust-knowledge-search", "ask", "Rust 所有权是什么?"]).unwrap();
        match cli.command {
            Command::Ask { question } => assert_eq!(question, "Rust 所有权是什么?"),
            other => panic!("expected Ask, got {other:?}"),
        }
    }
}
