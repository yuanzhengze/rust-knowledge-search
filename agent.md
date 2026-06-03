# Agent 协作规范

本文件用于约束本仓库中小组成员和 AI Agent 的协作方式。所有代码、文档和提交应围绕课程期末作业要求展开，保证项目可以运行、结构清晰、贡献明确，并能体现 Rust 语言核心特性。

## 1. 项目目标

本仓库项目为：基于 Rust 的本地知识库语义搜索系统。

项目应实现一个完整可运行的本地知识库检索工具，核心功能包括：

- 扫描本地 Markdown/TXT 文档。
- 解析文档内容并切分文本片段。
- 构建关键词索引。
- 提供命令行搜索功能。
- 可选提供语义搜索或 AI 问答增强。

项目重点不是简单调用 AI API，而是使用 Rust 完成文件处理、索引构建、搜索排序、错误处理、模块协作和工程化开发。

## 2. 基本原则

- 主要代码必须使用 Rust 编写。
- 项目必须具备完整、可运行的功能。
- 不得直接抄袭现成开源项目。
- 不做纯前端页面、简单 CRUD 或单纯 API 包装器。
- AI 生成代码只能作为辅助，必须经过成员理解、修改、测试和提交。
- 所有新增功能都应服务于最终项目目标，避免无关扩展。
- 优先完成稳定可运行的基础检索功能，再开发 AI 增强功能。

## 3. Rust 编码要求

代码应体现 Rust 工程实践：

- 合理使用 `struct`、`enum`、`trait` 和泛型。
- 合理体现 ownership 和 borrowing，不为绕过借用检查而滥用 clone。
- 统一使用 `Result` 处理可失败操作。
- 避免大量使用 `unwrap()` 和 `expect()` 规避错误处理。
- 对文件读取、目录扫描、索引加载、网络请求等错误给出清晰错误信息。
- 模块之间通过明确的数据结构和函数接口协作。
- 必要时使用并发或异步提升扫描、解析或请求效率。

## 4. 模块划分规范

推荐模块结构如下：

```text
src/
├── main.rs
├── cli.rs
├── scanner.rs
├── parser.rs
├── chunker.rs
├── indexer.rs
├── search.rs
├── semantic.rs
├── storage.rs
└── error.rs
```

模块职责：

- `cli.rs`：命令行参数解析和命令分发。
- `scanner.rs`：目录遍历、文件过滤和文件元数据读取。
- `parser.rs`：Markdown/TXT 文档读取和清洗。
- `chunker.rs`：文档切片和片段元数据维护。
- `indexer.rs`：倒排索引构建和索引数据结构。
- `search.rs`：关键词搜索、排序和结果生成。
- `semantic.rs`：语义搜索或 AI 问答增强。
- `storage.rs`：索引缓存保存与加载。
- `error.rs`：项目统一错误类型。

新增模块前应确认其职责不能由现有模块清晰承担。

## 5. 接口协作规范

跨模块数据结构应保持稳定。建议优先统一以下核心结构：

```rust
pub struct DocumentMeta {
    pub path: PathBuf,
    pub file_name: String,
    pub file_size: u64,
}

pub struct Document {
    pub meta: DocumentMeta,
    pub title: Option<String>,
    pub content: String,
}

pub struct DocumentChunk {
    pub id: String,
    pub file_path: PathBuf,
    pub title: Option<String>,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub struct SearchResult {
    pub chunk_id: String,
    pub file_path: PathBuf,
    pub snippet: String,
    pub score: f64,
}
```

如果需要修改公共结构或公共函数签名，必须同步相关成员，并更新测试和文档。

## 6. AI Agent 使用规范

使用 AI Agent 修改仓库时必须遵守：

- 先阅读相关文件和项目规范，再修改代码。
- 不要一次性重写无关模块。
- 不要删除成员已有代码，除非明确确认该代码已废弃。
- 不要引入与项目目标无关的大型依赖。
- 不要将 API Key、密钥、账号信息写入仓库。
- 不要为了编译通过而删除测试或弱化功能。
- 生成代码后必须检查是否符合 Rust 风格和本项目模块边界。
- 对关键逻辑应补充测试或说明测试原因。

AI 生成的内容应由负责成员审核后再合并。

## 7. Git 协作规范

项目必须体现 Git 协作过程和清晰 commit 历史。

推荐分支：

- `main`：稳定主分支。
- `feature/scanner`：文件扫描模块。
- `feature/parser`：文档解析模块。
- `feature/indexer`：索引构建模块。
- `feature/search`：搜索排序模块。
- `feature/cli`：命令行交互模块。
- `feature/semantic`：语义增强模块。
- `feature/docs`：文档、报告和演示材料。

提交信息建议：

```text
feat: add directory scanner
feat: implement document parser
feat: build inverted index
feat: add search command
fix: handle empty query error
test: add parser unit tests
docs: update usage guide
```

提交要求：

- 每次提交只包含一个相对完整的改动主题。
- 不提交 `target/` 目录。
- 不提交本地缓存、密钥、个人隐私文件。
- 合并前尽量保证项目可以编译运行。
- 每位成员应保留清晰可追踪的提交记录。

## 8. 测试与质量要求

提交前应尽量运行：

```bash
cargo fmt
cargo clippy
cargo test
```

至少包含：

- 单元测试。
- 若干关键功能测试。
- 对异常情况的测试或手动验证说明。

重点测试场景：

- 扫描正常目录。
- 扫描不存在目录。
- 解析 Markdown/TXT 文档。
- 切分长文档。
- 构建倒排索引。
- 搜索关键词并返回排序结果。
- 空查询和无结果查询。
- 索引缓存保存和加载。
- AI 不可用时 `ask` 命令可以合理降级。

## 9. 文档要求

仓库必须提供 README，内容至少包括：

- 项目简介。
- 功能列表。
- 环境要求。
- 编译方式。
- 运行方式。
- 示例命令。
- 项目结构。
- 成员分工。
- 依赖说明。

实验报告素材应覆盖：

- 选题背景。
- 需求分析。
- 总体设计。
- 模块设计。
- 核心实现。
- Rust 特性使用。
- 测试结果。
- 成员贡献。
- 总结与改进方向。

## 10. 成员贡献要求

本项目为 5 人组队项目，每位成员必须有明确贡献：

- 袁正泽：项目负责人、架构设计、`storage.rs`、`error.rs`、最终集成。
- 谭张锐：`scanner.rs` 文件扫描模块、`semantic.rs` 语义增强模块（Day 10–11 任务移交，2026-06-03 调整）。
- 邱俊杰：索引构建与搜索排序模块（`indexer.rs` + `search.rs`）。
- 黄开轩：CLI/TUI 交互与结果展示（`cli.rs` + `main.rs`，含 Day 9 体验优化）。
- 陈文涛：文档解析与切片（`parser.rs` + `chunker.rs`）、测试、README 和报告素材。

如果实际分工发生变化，应及时更新 README、协作文档和实验报告。

## 11. 功能优先级

开发顺序必须遵循：

1. 先完成可运行的基础版本。
2. 再优化搜索体验和缓存。
3. 最后实现语义搜索、AI 问答、TUI 或 PDF 解析等增强功能。

最小可交付版本必须包含：

- Markdown/TXT 文件扫描。
- 文档解析。
- 文档切片。
- 关键词索引。
- 命令行搜索。
- 搜索结果排序和展示。
- README。
- 测试。
- 成员分工说明。

## 12. 截止与提交规范

项目内部开发计划：

- 2026 年 5 月 31 日：确定选题、分组和项目初始化。
- 2026 年 6 月 6 日：完成最小可运行版本。
- 2026 年 6 月 10 日：完成语义增强或自然语言问答功能。
- 2026 年 6 月 13 日：完成测试、文档、报告素材和演示准备。

课程提交要求：

- 作业提交截止时间：6 月 26 日。
- 小组只需由组长提交。
- 提交内容包括项目源码或仓库链接、实验报告、5 分钟以内演示视频。
- 如果提交源码压缩包，不要包含 `target/` 目录。

## 13. 最终验收清单

最终提交前确认：

- 项目可以从零开始编译运行。
- `cargo fmt` 已执行。
- `cargo clippy` 尽量无明显警告。
- `cargo test` 中关键测试通过。
- README 完整。
- 实验报告素材完整。
- 演示视频流程已准备。
- 每位成员贡献清晰。
- Git commit 历史清晰。
- 没有提交 `target/`、密钥或隐私文件。
