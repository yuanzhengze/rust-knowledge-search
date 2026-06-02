//! 命令行入口 —— 用 [`clap`] 的 derive 模式定义子命令并分发。
//!
//! 负责人:黄开轩(见 `agent.md` §10、`plan.md` Day 9 / Day 11)。
//!
//! 命令布局(见 `docs/接口设计.md` §11):
//!
//! ```text
//! rust-knowledge-search index <path>
//! rust-knowledge-search search <query> [--top N]
//! rust-knowledge-search ask <question>
//! ```

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::error::AppResult;

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
///
/// 各分支的 todo 不影响 `--help` 输出和 CLI 路由,提交骨架时 `cargo run -- --help`
/// 即可正常列出三个子命令。
pub fn run() -> AppResult<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Index { path: _ } => {
            todo!(
                "串联 scanner::scan_documents → parser::parse_document → \
                 chunker::chunk_document → InvertedIndex::build → \
                 storage::save_index;输出扫描数 / 切片数 / 关键词数。\
                 见 docs/接口设计.md §1"
            )
        }
        Command::Search { query: _, top: _ } => {
            todo!(
                "storage::load_index → search::search → 渲染结果(路径 + 摘要 + 分数)。\
                 见 docs/接口设计.md §11"
            )
        }
        Command::Ask { question: _ } => {
            todo!(
                "1) storage::load_index;2) 先用 search 取候选;\
                 3) 若 SemanticEngine 可用调 answer;否则降级展示候选片段。\
                 见 agent.md §1.AI 可降级原则、docs/接口设计.md §10"
            )
        }
    }
}
