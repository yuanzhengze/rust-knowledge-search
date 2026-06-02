//! 集成测试占位 —— 对应 `docs/测试计划.md` §4 的三个场景。
//!
//! 当前全部以 `#[ignore]` 标记,让 `cargo test` 在骨架阶段通过的同时,
//! 给后续负责人留下明确的"启用我"标记。每个测试上面的注释指明依赖哪些模块完成。

use clap::Parser;
use rust_knowledge_search::{cli, AppResult};

/// 让 cli 模块至少在编译期被引用,防止仅使用 binary 时被 dead-code lint 警告。
#[allow(dead_code)]
fn _types_compile() -> AppResult<()> {
    let _ = cli::Cli::try_parse_from(["rust-knowledge-search", "search", "x"]);
    Ok(())
}

#[test]
#[ignore = "skeleton: 待 indexer + search 完成后启用 —— 测试计划.md §4 完整索引流程"]
fn index_and_search_sample_notes() {
    // 1. 调 cli::run() 或直接组合 scanner→parser→chunker→indexer 处理 examples/sample_notes/
    // 2. 用关键词 "Rust" 搜索,断言至少返回一条结果且分数 > 0
}

#[test]
#[ignore = "skeleton: 待 storage 完成后启用 —— 测试计划.md §4 缓存复用流程"]
fn search_after_loading_cached_index() {
    // 1. save_index 到临时文件
    // 2. load_index 读回
    // 3. 在加载后的索引上做一次 search,断言结果与构建时一致
}

#[test]
#[ignore = "skeleton: 待 cli + semantic 完成后启用 —— 测试计划.md §4 ask 降级流程"]
fn ask_falls_back_when_ai_unavailable() {
    // 1. 在没有 AI_API_KEY 的环境下运行 ask 子命令
    // 2. 断言不 panic、不 Err,且输出包含至少一个候选片段
}
