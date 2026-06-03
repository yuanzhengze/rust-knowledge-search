//! 集成测试 —— 对应 `docs/测试计划.md` §4 的三个场景。
//!
//! 这些测试通过 `cli::run_*_at` 暴露的库函数模拟真实子命令的行为,
//! 同时直接组合下游模块(scanner/parser/chunker/indexer/search/storage)
//! 验证端到端正确性。
//!
//! 缓存路径全部使用进程局部的临时目录,避免污染当前工作目录。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use rust_knowledge_search::{
    chunker::{chunk_document, DocumentChunk},
    cli,
    indexer::InvertedIndex,
    parser::parse_document,
    scanner::scan_documents,
    search::search,
    storage::{load_index, save_index},
    AppResult,
};

/// 让 cli 模块至少在编译期被引用,防止仅使用 binary 时被 dead-code lint 警告。
#[allow(dead_code)]
fn _types_compile() -> AppResult<()> {
    let _ = cli::Cli::try_parse_from(["rust-knowledge-search", "search", "x"]);
    Ok(())
}

/// 零依赖临时目录:与 src/ 各模块单测里 TempDir 同款。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rks-it-{}-{}-{}-{}",
            label,
            std::process::id(),
            nanos,
            n
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn sample_notes_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/sample_notes")
}

/// 工具:把 sample_notes 跑一遍 scanner→parser→chunker→indexer 链,
/// 返回 (索引, chunks) 二元组,供需要内部状态的测试断言。
fn build_sample_artifacts() -> (InvertedIndex, Vec<DocumentChunk>) {
    let metas = scan_documents(&sample_notes_dir()).expect("scan");
    let mut chunks = Vec::new();
    for meta in metas {
        let doc = parse_document(meta).expect("parse");
        chunks.extend(chunk_document(&doc, 200));
    }
    let index = InvertedIndex::build(&chunks);
    (index, chunks)
}

#[test]
fn index_and_search_sample_notes() {
    // 测试计划 §4 第一条:对示例目录执行完整索引流程,然后用关键词检索。
    let dir = TempDir::new("index_search");
    let cache = dir.path().join("idx.json");

    // 1. 通过 cli 库函数走完整 index 子命令流水线。
    cli::run_index_at(&sample_notes_dir(), &cache).expect("index command should succeed");
    assert!(cache.exists(), "index 后缓存文件应当存在");

    // 2. 通过 cli 库函数走 search 子命令(只验证不 panic / 不 Err,降级路径走通)。
    //    集成测试统一用 color=false,避免 ANSI 转义码污染测试输出。
    cli::run_search_at(&cache, "Rust", 5, false).expect("search command should succeed");

    // 3. 用底层 API 复检搜索结果,断言至少一条且分数 > 0。
    let (index, chunks) = build_sample_artifacts();
    let results = search(&index, &chunks, "Rust", 5).expect("search ok");
    assert!(!results.is_empty(), "Rust 应当至少命中一条");
    assert!(
        results.iter().all(|r| r.score > 0.0),
        "命中结果分数必须 > 0"
    );
    // 中文查询也应当命中。
    let zh = search(&index, &chunks, "所有权", 5).expect("search ok");
    assert!(!zh.is_empty(), "「所有权」应当至少命中一条");
}

#[test]
fn search_after_loading_cached_index() {
    // 测试计划 §4 第二条:save_index → load_index → 在加载后的索引上做 search,
    // 结果应与构建时一致。
    let dir = TempDir::new("cache_reuse");
    let cache = dir.path().join("idx.json");

    let (index, chunks) = build_sample_artifacts();
    save_index(&cache, &index, &chunks).expect("save");

    let (loaded_index, loaded_chunks) = load_index(&cache).expect("load");
    assert_eq!(loaded_chunks.len(), chunks.len(), "chunks 数量应保持");

    // 同一查询在原索引和加载后索引上,结果集合应当一致(顺序受同分 tie-break 影响,
    // 但 chunk_id 集合不应变化)。
    let original = search(&index, &chunks, "Rust", 10).expect("search original");
    let restored = search(&loaded_index, &loaded_chunks, "Rust", 10).expect("search restored");
    assert_eq!(original.len(), restored.len(), "命中数量应当一致");
    let original_ids: Vec<_> = original.iter().map(|r| r.chunk_id.as_str()).collect();
    let restored_ids: Vec<_> = restored.iter().map(|r| r.chunk_id.as_str()).collect();
    assert_eq!(original_ids, restored_ids, "命中 chunk_id 序列应一致");
    for (o, r) in original.iter().zip(restored.iter()) {
        assert!(
            (o.score - r.score).abs() < f64::EPSILON,
            "分数应一致: {} vs {}",
            o.score,
            r.score
        );
    }
}

#[test]
fn ask_falls_back_when_ai_unavailable() {
    // 测试计划 §4 第三条:在没有 AI_API_KEY 的环境下运行 ask 子命令,
    // 不 panic / 不 Err,且会降级到候选片段展示路径。
    let dir = TempDir::new("ask_fallback");
    let cache = dir.path().join("idx.json");

    // 准备索引(走 cli 库函数,模拟用户先 index 再 ask)。
    cli::run_index_at(&sample_notes_dir(), &cache).expect("index");

    // ask 命令在 NoopEngine 下应当走"AI 不可用 → 展示候选片段"分支,返回 Ok。
    cli::run_ask_at(&cache, "Rust 的所有权是什么?", false)
        .expect("ask should not error when AI unavailable");

    // 即便问题完全无关,也应当走 Ok 而不是 Err(可能返回 0 条候选,但不崩)。
    cli::run_ask_at(&cache, "kotlin coroutines", false)
        .expect("ask should handle no-match gracefully");
}

#[test]
fn search_with_color_should_emit_ansi_in_snippet() {
    // Day 9 验收:cli::build_snippet 在 color=true 时应在 snippet 上打 ANSI 高亮码。
    // 集成测试层面间接断言 cli 模块对外暴露的 build_snippet 函数行为。
    let snippet = cli::build_snippet("Rust 所有权 是核心", &["rust".to_string()], true);
    assert!(
        snippet.contains("\x1b["),
        "color=true 时 snippet 应当含 ANSI 控制码,实际: {snippet:?}"
    );

    let plain = cli::build_snippet("Rust 所有权 是核心", &["rust".to_string()], false);
    assert!(
        !plain.contains("\x1b["),
        "color=false 时 snippet 不应含 ANSI 控制码,实际: {plain:?}"
    );
}

#[test]
fn pipeline_should_handle_special_filenames_end_to_end() {
    // Day 12 验收:含 emoji / 中文 / 空格的文件名能完整流过
    // scanner → parser → chunker → indexer → search → storage 整条流水线。
    let dir = TempDir::new("special_names");
    let knowledge_root = dir.path().join("knowledge");
    fs::create_dir_all(&knowledge_root).unwrap();

    fs::write(
        knowledge_root.join("📝note.md"),
        "# Emoji 文件名\n\nRust 所有权 emoji-marked\n",
    )
    .unwrap();
    fs::write(
        knowledge_root.join("学习笔记.md"),
        "# 中文文件名\n\nRust 错误处理 chinese-marked\n",
    )
    .unwrap();
    fs::write(
        knowledge_root.join("my notes.txt"),
        "Spaced filename\n包含 Rust 关键词\n",
    )
    .unwrap();

    let cache = dir.path().join("idx.json");
    cli::run_index_at(&knowledge_root, &cache).expect("index special filenames");
    assert!(cache.exists(), "缓存应当生成");

    // search 应当能跨三个特殊文件名命中 Rust
    let (index, chunks) = rust_knowledge_search::storage::load_index(&cache).expect("load");
    let results = search(&index, &chunks, "Rust", 10).expect("search");
    assert!(
        results.len() >= 3,
        "三个特殊文件名都含 Rust,应至少 3 条命中,实际 {}",
        results.len()
    );
    let chunk_ids: Vec<_> = results.iter().map(|r| r.chunk_id.as_str()).collect();
    assert!(
        chunk_ids.iter().any(|id| id.contains("📝note")),
        "emoji 文件应当被索引"
    );
    assert!(
        chunk_ids.iter().any(|id| id.contains("学习笔记")),
        "中文文件应当被索引"
    );
    assert!(
        chunk_ids.iter().any(|id| id.contains("my notes")),
        "含空格文件应当被索引"
    );

    // 通过 cli::run_search_at 走完整渲染路径(color=false 让输出可断言)
    cli::run_search_at(&cache, "Rust", 10, false).expect("search via cli");
}
