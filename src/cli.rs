//! 命令行入口 —— 用 [`clap`] 的 derive 模式定义子命令并分发。
//!
//! 负责人:黄开轩(见 `agent.md` §10、`plan.md` Day 7 + Day 9)。
//!
//! 命令布局(见 `docs/接口设计.md` §11):
//!
//! ```text
//! rust-knowledge-search index <path>
//! rust-knowledge-search search <query> [--top N]
//! rust-knowledge-search ask <question>
//! ```
//!
//! 全局参数:
//! - `--color <auto|always|never>` 控制 ANSI 着色(默认 auto)。
//!   auto 模式下检查 `NO_COLOR` 环境变量(事实标准 https://no-color.org)
//!   与 stdout 是否为终端,管道/重定向场景自动关闭。
//!
//! 设计要点:
//! - 真正的业务逻辑放在 [`run_index_at`] / [`run_search_at`] / [`run_ask_at`]
//!   三个 `*_at` 函数里,显式接收 `cache_path` + `color` 参数;[`run`] 只做
//!   clap 解析、环境变量解析和 TTY 探测。集成测试可以传临时缓存路径并固定
//!   `color = false` 让输出不被 ANSI 污染。
//! - 缓存路径优先级:`KNOWLEDGE_INDEX_PATH` 环境变量 > 默认 `.knowledge_index.json`。
//! - `ask` 命令在 [`SemanticEngine::answer`] 返回 [`AppError::Semantic`] 时降级
//!   展示候选片段(`agent.md` §1 的"AI 可降级"原则)。
//! - snippet 智能上下文(Day 9 体验优化):围绕首个匹配 token 前后各
//!   [`SNIPPET_RADIUS`] 个字符取 snippet,而不是简单截断开头。

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::chunker::{chunk_document, DocumentChunk};
use crate::error::{AppError, AppResult};
use crate::indexer::InvertedIndex;
use crate::parser::parse_document;
use crate::scanner::scan_documents;
use crate::search::{search, SearchResult};
use crate::semantic::{ChatEngine, NoopEngine, SemanticEngine};
use crate::storage::{load_index, save_index};

/// 默认缓存文件名(相对当前工作目录),与 `.gitignore` 保持一致。
const DEFAULT_CACHE: &str = ".knowledge_index.json";
/// 缓存路径环境变量名,见 `.env.example`。
const CACHE_ENV: &str = "KNOWLEDGE_INDEX_PATH";
/// 文档切片的字符上限,与 `chunker` 单测里使用的值保持一致。
const DEFAULT_CHUNK_CHARS: usize = 200;
/// `ask` 命令默认拉取的候选片段数。
const ASK_TOP_K: usize = 5;
/// snippet 围绕匹配位置取上下文的"半径"(前后各取多少字符)。
const SNIPPET_RADIUS: usize = 60;
/// 截断时使用的省略号字符。
const ELLIPSIS: char = '…';

// ---------------------------------------------------------------------------
// ANSI 颜色辅助
// ---------------------------------------------------------------------------

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_YELLOW_BOLD: &str = "\x1b[33;1m";
const ANSI_BLUE: &str = "\x1b[34m";
const ANSI_GREEN: &str = "\x1b[32m";

/// `--color` 选项支持的三种模式。
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum ColorChoice {
    /// 自动:stdout 是 TTY 且没有 `NO_COLOR` 时启用。
    #[default]
    Auto,
    /// 始终输出 ANSI 着色,即使被重定向。
    Always,
    /// 始终关闭 ANSI 着色,适合管道、CI 与日志归档。
    Never,
}

/// 解析最终是否启用着色。`auto` 模式遵守 `NO_COLOR` 事实标准与 TTY 检测。
pub fn color_enabled(choice: ColorChoice) -> bool {
    color_enabled_with(choice, std::env::var_os("NO_COLOR").is_some(), || {
        std::io::stdout().is_terminal()
    })
}

/// 抽出来的纯函数版本,便于单测覆盖各种环境组合。
fn color_enabled_with(
    choice: ColorChoice,
    no_color_env: bool,
    is_tty: impl FnOnce() -> bool,
) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => !no_color_env && is_tty(),
    }
}

/// 在 `enabled = true` 时把 ANSI 控制码包到 `text` 上,否则原样返回。
fn paint(text: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{text}{ANSI_RESET}")
    } else {
        text.to_string()
    }
}

// ---------------------------------------------------------------------------
// CLI 解析
// ---------------------------------------------------------------------------

/// 顶层 CLI 解析器。
#[derive(Debug, Parser)]
#[command(
    name = "rust-knowledge-search",
    version,
    about = "基于 Rust 的本地知识库语义搜索系统",
    long_about = "扫描本地 Markdown/TXT 文档,构建关键词索引,支持关键词搜索与可选的自然语言问答。"
)]
pub struct Cli {
    /// 控制终端 ANSI 着色。
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true)]
    pub color: ColorChoice,

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
    let color = color_enabled(cli.color);
    match cli.command {
        Command::Index { path } => run_index_at(&path, &cache),
        Command::Search { query, top } => run_search_at(&cache, &query, top, color),
        Command::Ask { question } => run_ask_at(&cache, &question, color),
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

// ---------------------------------------------------------------------------
// 子命令实现
// ---------------------------------------------------------------------------

/// `index` 子命令实现:扫描目录 → 解析 → 切片 → 构建索引 → 写入缓存。
///
/// `index` 命令本身没有需要着色的内容(纯统计输出),所以不接收 `color` 参数。
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

/// `search` 子命令实现:加载缓存 → 检索 → 渲染(可选着色 + 高亮 snippet)。
pub fn run_search_at(cache: &Path, query: &str, top: usize, color: bool) -> AppResult<()> {
    let (index, chunks) = load_index(cache)?;
    let results = search(&index, &chunks, query, top)?;

    if results.is_empty() {
        println!(
            "未找到与「{}」相关的文档片段。",
            paint(query, ANSI_BOLD, color)
        );
        return Ok(());
    }

    let chunk_lookup: HashMap<&str, &DocumentChunk> =
        chunks.iter().map(|c| (c.id.as_str(), c)).collect();

    println!(
        "{}\n",
        paint(&format!("找到 {} 条结果:", results.len()), ANSI_BOLD, color)
    );
    for (i, r) in results.iter().enumerate() {
        render_one_result(i + 1, r, &chunk_lookup, color);
    }
    Ok(())
}

/// `ask` 子命令实现:加载缓存 → 检索候选 → 调用 [`SemanticEngine::answer`],
/// 失败时降级展示候选片段。
///
/// 引擎选择策略:
/// 1. 先尝试 [`ChatEngine::from_env`] —— 只要 `AI_API_KEY` 配置好就用真实 LLM。
/// 2. 没配置就直接用 [`NoopEngine`],它的 `answer` 永远返回
///    `AppError::Semantic("semantic engine disabled")`,被下面的 match 捕获后降级。
/// 3. 即便 `ChatEngine` 在调用过程中网络失败,错误也是 `AppError::Semantic(...)`,
///    依然走降级分支,不会 panic 也不会让整个进程退出。
pub fn run_ask_at(cache: &Path, question: &str, color: bool) -> AppResult<()> {
    let (index, chunks) = load_index(cache)?;
    let results = search(&index, &chunks, question, ASK_TOP_K)?;

    if results.is_empty() {
        println!("未找到与该问题相关的文档片段。");
        return Ok(());
    }

    let context_chunks: Vec<DocumentChunk> = chunks
        .iter()
        .filter(|c| results.iter().any(|r| r.chunk_id == c.id))
        .cloned()
        .collect();

    // 优先 ChatEngine,失败则 NoopEngine。两者都返回 AppResult<String>,
    // 下面的 match 统一处理。
    let answer_result = if let Some(engine) = ChatEngine::from_env() {
        engine.answer(question, &context_chunks)
    } else {
        NoopEngine.answer(question, &context_chunks)
    };

    match answer_result {
        Ok(answer) => {
            println!("{}", paint("AI 回答:", ANSI_BOLD, color));
            println!("{}\n", answer.trim());
            println!("{}\n", paint("引用片段:", ANSI_BOLD, color));
        }
        Err(AppError::Semantic(msg)) => {
            println!(
                "{} AI 引擎不可用 ({})。以下是检索到的相关片段:\n",
                paint("[提示]", ANSI_DIM, color),
                msg
            );
        }
        Err(other) => return Err(other),
    }

    let chunk_lookup: HashMap<&str, &DocumentChunk> =
        chunks.iter().map(|c| (c.id.as_str(), c)).collect();
    for (i, r) in results.iter().enumerate() {
        render_one_result(i + 1, r, &chunk_lookup, color);
    }
    Ok(())
}

/// 把单条搜索结果按统一格式渲染到 stdout。
fn render_one_result(
    index: usize,
    r: &SearchResult,
    chunk_lookup: &HashMap<&str, &DocumentChunk>,
    color: bool,
) {
    let snippet = chunk_lookup
        .get(r.chunk_id.as_str())
        .map(|c| build_snippet(&c.content, &r.matched_terms, color))
        .unwrap_or_else(|| r.snippet.clone());

    println!(
        "{} {}  {}",
        paint(&format!("[{}]", index), ANSI_BOLD, color),
        paint(&r.chunk_id, ANSI_BLUE, color),
        paint(&format!("分数 {:.2}", r.score), ANSI_GREEN, color),
    );
    if !r.matched_terms.is_empty() {
        let terms_str = r
            .matched_terms
            .iter()
            .map(|t| paint(t, ANSI_YELLOW_BOLD, color))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "    {} {}",
            paint("匹配关键词:", ANSI_DIM, color),
            terms_str
        );
    }
    println!("    {}", snippet);
    println!();
}

// ---------------------------------------------------------------------------
// snippet 智能上下文 + 高亮
// ---------------------------------------------------------------------------

/// 围绕首个匹配 token 取上下文构建 snippet,并对所有 matched_terms 做高亮。
///
/// 算法:
/// 1. 把 `content` 与各 term 都按 char(而非 byte)处理,UTF-8 安全。
/// 2. 找出 `matched_terms` 中第一个在 content(忽略大小写)出现的位置,
///    取该位置前后各 [`SNIPPET_RADIUS`] 个字符作为 snippet。
/// 3. 没有匹配时退化为取开头 `2 * SNIPPET_RADIUS` 个字符。
/// 4. 如果起始或结束被截断,在对应位置加 [`ELLIPSIS`]。
/// 5. 在 snippet 上对所有 term 做大小写不敏感的高亮(仅在 `color = true` 时)。
pub fn build_snippet(content: &str, matched_terms: &[String], color: bool) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    let total = chars.len();

    // 找 matched_terms 中最早出现的位置。
    let earliest_match: Option<usize> = matched_terms
        .iter()
        .filter_map(|term| find_term_in(&lower, term))
        .min();

    let max_window = SNIPPET_RADIUS * 2;
    let (start, end) = match earliest_match {
        Some(pos) => {
            let start = pos.saturating_sub(SNIPPET_RADIUS);
            let end = (pos + SNIPPET_RADIUS).min(total);
            (start, end)
        }
        None => (0, max_window.min(total)),
    };

    let mut snippet: String = chars[start..end].iter().collect();
    if start > 0 {
        snippet.insert(0, ELLIPSIS);
    }
    if end < total {
        snippet.push(ELLIPSIS);
    }

    if color {
        for term in matched_terms {
            snippet = highlight_all(&snippet, term);
        }
    }
    snippet
}

/// 在 `lower_chars`(已小写)中按字符朴素查找 `term`,返回首次出现的字符索引。
fn find_term_in(lower_chars: &[char], term: &str) -> Option<usize> {
    let term_chars: Vec<char> = term.chars().collect();
    if term_chars.is_empty() || term_chars.len() > lower_chars.len() {
        return None;
    }
    'outer: for i in 0..=lower_chars.len() - term_chars.len() {
        for j in 0..term_chars.len() {
            if lower_chars[i + j] != term_chars[j] {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

/// 在 `snippet` 中把所有(大小写不敏感)出现的 `term` 用 ANSI 黄色加粗包起来。
/// 已经包含 ANSI 控制码的位置不会被二次包裹(我们按字符跳过 `\x1b` 序列)。
fn highlight_all(snippet: &str, term: &str) -> String {
    if term.is_empty() {
        return snippet.to_string();
    }
    let snippet_chars: Vec<char> = snippet.chars().collect();
    let lower_snippet: Vec<char> = snippet_chars
        .iter()
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let term_chars: Vec<char> = term.chars().collect();
    let term_len = term_chars.len();
    if term_len == 0 || term_len > snippet_chars.len() {
        return snippet.to_string();
    }

    let mut result = String::with_capacity(snippet.len() + 16);
    let mut i = 0;
    let n = snippet_chars.len();
    while i < n {
        // 跳过已有的 ANSI 控制序列(从 ESC 到 'm'),避免把控制码内部当作匹配。
        if snippet_chars[i] == '\x1b' {
            result.push(snippet_chars[i]);
            i += 1;
            while i < n && snippet_chars[i] != 'm' {
                result.push(snippet_chars[i]);
                i += 1;
            }
            if i < n {
                result.push(snippet_chars[i]); // 'm'
                i += 1;
            }
            continue;
        }
        // 检查从 i 起 term_len 个字符是否(忽略大小写)与 term 相等。
        if i + term_len <= n && (0..term_len).all(|j| lower_snippet[i + j] == term_chars[j]) {
            result.push_str(ANSI_YELLOW_BOLD);
            for j in 0..term_len {
                result.push(snippet_chars[i + j]);
            }
            result.push_str(ANSI_RESET);
            i += term_len;
        } else {
            result.push(snippet_chars[i]);
            i += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------- cache_path_from_env ----------------

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
        let path = cache_path_from_env(Some(""));
        assert_eq!(path, PathBuf::from(DEFAULT_CACHE));
    }

    // ---------------- color_enabled_with ----------------

    #[test]
    fn color_always_should_force_enabled() {
        assert!(color_enabled_with(ColorChoice::Always, true, || false));
        assert!(color_enabled_with(ColorChoice::Always, false, || false));
    }

    #[test]
    fn color_never_should_force_disabled() {
        assert!(!color_enabled_with(ColorChoice::Never, false, || true));
        assert!(!color_enabled_with(ColorChoice::Never, true, || true));
    }

    #[test]
    fn color_auto_should_disable_when_no_color_env_set() {
        assert!(!color_enabled_with(ColorChoice::Auto, true, || true));
    }

    #[test]
    fn color_auto_should_disable_when_not_tty() {
        assert!(!color_enabled_with(ColorChoice::Auto, false, || false));
    }

    #[test]
    fn color_auto_should_enable_when_tty_and_no_no_color_env() {
        assert!(color_enabled_with(ColorChoice::Auto, false, || true));
    }

    // ---------------- paint ----------------

    #[test]
    fn paint_should_wrap_when_enabled() {
        let s = paint("hello", ANSI_BOLD, true);
        assert!(s.starts_with(ANSI_BOLD));
        assert!(s.ends_with(ANSI_RESET));
        assert!(s.contains("hello"));
    }

    #[test]
    fn paint_should_passthrough_when_disabled() {
        assert_eq!(paint("hello", ANSI_BOLD, false), "hello");
    }

    // ---------------- build_snippet ----------------

    #[test]
    fn build_snippet_should_center_on_first_match() {
        let content = "x".repeat(100) + " HIT word " + &"y".repeat(100);
        let terms = vec!["hit".to_string()];
        let snippet = build_snippet(&content, &terms, false);

        assert!(snippet.contains("HIT"), "snippet 应包含原大小写的匹配词");
        // 由于 SNIPPET_RADIUS=60,snippet 长度应当 <= 121(含两端 ELLIPSIS)
        assert!(
            snippet.chars().count() <= SNIPPET_RADIUS * 2 + 2,
            "snippet 太长: {}",
            snippet.chars().count()
        );
        assert!(snippet.starts_with(ELLIPSIS), "起始应有省略号");
        assert!(snippet.ends_with(ELLIPSIS), "结尾应有省略号");
    }

    #[test]
    fn build_snippet_should_fall_back_to_head_when_no_match() {
        let content = "a".repeat(300);
        let terms = vec!["nope".to_string()];
        let snippet = build_snippet(&content, &terms, false);

        assert!(
            !snippet.starts_with(ELLIPSIS),
            "无匹配时取开头不应有前导省略号"
        );
        assert!(snippet.ends_with(ELLIPSIS), "结尾被截断应有省略号");
        assert!(snippet.chars().count() <= SNIPPET_RADIUS * 2 + 1);
    }

    #[test]
    fn build_snippet_should_not_truncate_short_content() {
        let content = "Rust 所有权";
        let terms = vec!["所".to_string()];
        let snippet = build_snippet(content, &terms, false);
        assert!(!snippet.starts_with(ELLIPSIS));
        assert!(!snippet.ends_with(ELLIPSIS));
        assert!(snippet.contains("Rust"));
    }

    #[test]
    fn build_snippet_should_handle_empty_content() {
        assert_eq!(build_snippet("", &["x".to_string()], false), "");
        assert_eq!(build_snippet("   \n\t  ", &["x".to_string()], false), "");
    }

    #[test]
    fn build_snippet_should_apply_highlight_when_color_enabled() {
        let content = "Rust 所有权 是核心";
        let terms = vec!["rust".to_string(), "权".to_string()];
        let snippet = build_snippet(content, &terms, true);
        assert!(snippet.contains(ANSI_YELLOW_BOLD), "应当包含高亮 ANSI 码");
        assert!(snippet.contains(ANSI_RESET));
    }

    #[test]
    fn build_snippet_should_skip_highlight_when_color_disabled() {
        let content = "Rust 所有权";
        let terms = vec!["rust".to_string()];
        let snippet = build_snippet(content, &terms, false);
        assert!(!snippet.contains(ANSI_YELLOW_BOLD));
        assert!(!snippet.contains("\x1b["));
    }

    // ---------------- highlight_all ----------------

    #[test]
    fn highlight_should_wrap_matched_terms() {
        let s = highlight_all("Rust is great", "rust");
        assert_eq!(s, format!("{ANSI_YELLOW_BOLD}Rust{ANSI_RESET} is great"));
    }

    #[test]
    fn highlight_should_be_case_insensitive() {
        let s = highlight_all("RUST and rust and RuSt", "rust");
        // 三个匹配,每个都应被包裹
        assert_eq!(s.matches(ANSI_YELLOW_BOLD).count(), 3);
        assert_eq!(s.matches(ANSI_RESET).count(), 3);
    }

    #[test]
    fn highlight_should_handle_cjk() {
        let s = highlight_all("Rust 所有权 借用", "权");
        assert!(s.contains(&format!("{ANSI_YELLOW_BOLD}权{ANSI_RESET}")));
    }

    #[test]
    fn highlight_should_no_op_for_empty_term() {
        assert_eq!(highlight_all("Rust", ""), "Rust");
    }

    #[test]
    fn highlight_should_no_op_for_term_longer_than_text() {
        assert_eq!(highlight_all("ab", "abcdef"), "ab");
    }

    #[test]
    fn highlight_should_skip_existing_ansi_sequences() {
        // 已经包过一次的 snippet 再叠一次不应破坏首层包裹。
        let once = highlight_all("Rust word", "rust");
        let twice = highlight_all(&once, "word");
        // word 也应该被高亮,首层 Rust 高亮保留。
        assert!(twice.contains("Rust"));
        assert!(twice.contains("word"));
        assert!(twice.contains(ANSI_YELLOW_BOLD));
    }

    // ---------------- find_term_in ----------------

    #[test]
    fn find_term_in_returns_first_position() {
        let chars: Vec<char> = "hello world hello".chars().collect();
        assert_eq!(find_term_in(&chars, "hello"), Some(0));
        assert_eq!(find_term_in(&chars, "world"), Some(6));
        assert_eq!(find_term_in(&chars, "nope"), None);
    }

    // ---------------- clap parsing ----------------

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

    #[test]
    fn cli_parses_color_flag() {
        let cli = Cli::try_parse_from([
            "rust-knowledge-search",
            "--color",
            "never",
            "search",
            "Rust",
        ])
        .unwrap();
        assert!(matches!(cli.color, ColorChoice::Never));
    }

    #[test]
    fn cli_color_defaults_to_auto() {
        let cli = Cli::try_parse_from(["rust-knowledge-search", "search", "Rust"]).unwrap();
        assert!(matches!(cli.color, ColorChoice::Auto));
    }
}
