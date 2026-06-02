# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

This repository is a **Rust course final project with the MVP completed** (as of 2026-06-02). The Cargo skeleton is in place, all eight production modules under `src/` are implemented, and **65 unit tests + 3 integration tests** pass with `cargo clippy --all-targets -- -D warnings` clean. The three CLI subcommands (`index` / `search` / `ask`) work end-to-end against `examples/sample_notes/`, and `ask` gracefully degrades to candidate snippets when no AI backend is configured (the default `NoopEngine` always returns `AppError::Semantic`).

The project is "基于 Rust 的本地知识库语义搜索系统" — a CLI tool that scans a local directory of Markdown/TXT notes, builds an inverted index, and answers keyword queries plus optional natural-language questions. Most documentation is in Chinese; mirror that language when editing existing docs and in user-facing CLI output, but keep code identifiers in English.

Remaining work (per `plan.md` Day 9–14, all post-MVP polish):

- Day 9 (黄开轩): keyword highlighting, smarter snippet windowing, ANSI colors.
- Day 10–11 (袁正泽): real `SemanticEngine` implementation — wire `AI_API_KEY` / `AI_BASE_URL` env vars to an embedding or chat API.
- Day 12 (all): boundary tests (symlinks, permission denied, BOM, oversized files).
- Day 13 (陈文涛): README / 实验报告素材 / 演示脚本 are already drafted; final polish before submission.
- Day 14 (all): demo video recording and final acceptance.

## Common commands

Once the Cargo project exists, the standard development loop is:

```bash
cargo build
cargo fmt
cargo clippy
cargo test                       # all tests
cargo test <test_name>           # single test by name (substring match)
cargo test --test integration_test   # a specific integration test file under tests/
```

Planned CLI surface (see `docs/接口设计.md` §11):

```bash
cargo run -- index ./examples/sample_notes
cargo run -- search "Rust 所有权"
cargo run -- search "索引" --top 5
cargo run -- ask "Rust 的所有权是什么？"
```

`examples/sample_notes/` is the canonical fixture directory used by both manual demos and integration tests. Do not delete or rename it without updating `docs/演示脚本.md` and `docs/测试计划.md`.

## Architecture: required module layout

The pipeline is fixed and several teammates own specific modules — do **not** invent a different module split or move responsibilities between modules without first updating `agent.md` and `docs/接口设计.md`. The data flow is:

```
CLI → scanner → parser → chunker → indexer → search → (optional) semantic → CLI output
                                              ↑↓
                                            storage (cache)
```

| Module | Responsibility | Owner |
|---|---|---|
| `cli.rs` + `main.rs` | `clap`-based arg parsing, command dispatch, result rendering | 黄开轩 |
| `scanner.rs` | Recursive directory walk, filter `.md`/`.txt`, gather `DocumentMeta` only — does **not** read full file contents | 谭张锐 |
| `parser.rs` | Read file, clean Markdown/TXT, extract title → `Document` | 陈文涛 |
| `chunker.rs` | Split `Document` into `DocumentChunk`s with stable IDs and line ranges | 陈文涛 |
| `indexer.rs` | Build `InvertedIndex` (term → chunk-id mapping + term frequencies) | 邱俊杰 |
| `search.rs` | Score and rank chunks, return top-k `SearchResult`s | 邱俊杰 |
| `semantic.rs` | Optional `SemanticEngine` trait — embedding/AI ranking and `ask` answering | 袁正泽 |
| `storage.rs` | `serde_json` save/load of index + chunks to a cache file | 袁正泽 |
| `error.rs` | `AppError` (thiserror) and `AppResult<T>` used by every module | 袁正泽 |

## Core data structures

These types cross every module boundary. Their shapes are pinned in `docs/接口设计.md` §2; treat them as a contract and do not change a field without coordinating with the affected module owner. Cliff-notes form (full definitions in the design doc):

- `DocumentMeta { path, file_name, file_size, modified_time }`
- `Document { meta, title: Option<String>, content }`
- `DocumentChunk { id, file_path, title, content, start_line, end_line }`
- `SearchResult { chunk_id, file_path, snippet, score, matched_terms }`
- `AppError` (thiserror enum) + `pub type AppResult<T> = Result<T, AppError>;`

All fallible functions return `AppResult<T>`. Avoid `unwrap()`/`expect()` outside tests — `agent.md` calls this out specifically.

## Non-negotiable project rules

These come from `agent.md` and override default Claude habits:

- **AI is an enhancement, not the core.** Keyword search via `scanner → parser → chunker → indexer → search` must work end-to-end with no API key, no network, and no `semantic` module. The `ask` command must gracefully fall back to showing retrieved chunks when the AI backend is unavailable.
- **Don't add large or off-topic dependencies.** Planned deps are `clap`, `walkdir`, `serde`, `serde_json`, `thiserror`, `regex`; optional `tokio`, `reqwest`, `ratatui`, `crossterm`. Anything outside this set needs justification.
- **Don't rewrite teammates' modules.** When making a change, edit only the module(s) that own the responsibility. If a public signature in one module needs to change, update `docs/接口设计.md` in the same change and note it in the commit message.
- **Keep `Cargo.lock` committed** (this is a binary application — the `.gitignore` is intentionally configured to keep it).
- **Never commit** `target/`, `.env`, the local `.knowledge_index.json` cache, or anything under `private/` or `secrets/`. `.env.example` documents the AI-related env vars (`AI_API_KEY`, `AI_BASE_URL`, `AI_EMBEDDING_MODEL`, `AI_CHAT_MODEL`, `KNOWLEDGE_INDEX_PATH`) — read them from the environment in `semantic.rs`, never hard-code.
- **Build the minimum viable version first** (scan → parse → chunk → index → search → CLI) before touching `semantic.rs`, caching, highlighting, or TUI. `plan.md` and `agent.md` §11 describe the prioritization.

## Testing expectations

`docs/测试计划.md` lists the required test cases per module — when implementing or modifying a module, check that file for the expected test names and add them. Required exception coverage: missing path, empty directory, empty query, non-UTF-8 file, corrupted cache, `ask` without API key.

## Git workflow

`CONTRIBUTING.md` defines the branch/commit conventions. Feature branches are `feature/<module>` (e.g. `feature/scanner`, `feature/indexer`). Commit messages use conventional prefixes: `feat:`, `fix:`, `test:`, `docs:`, `refactor:`, `chore:`. One logical change per commit; run `cargo fmt && cargo clippy && cargo test` before merging to `main`.

## Doc sync rules

When you change behavior, update the matching doc in the same commit:

- CLI flags or commands → `README.md` and `docs/演示脚本.md`
- Public structs / function signatures → `docs/接口设计.md`
- Test approach → `docs/测试计划.md`
- Member responsibilities → `README.md` and `项目协作文档.md`
- Stage milestones → `开发日志.md`
