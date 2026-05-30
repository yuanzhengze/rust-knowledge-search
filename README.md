# 基于 Rust 的本地知识库语义搜索系统

本项目是 Rust 程序设计课程期末大作业，目标是实现一个本地知识库智能检索工具。用户可以指定本地资料目录，系统自动扫描 Markdown/TXT 文档，解析内容并建立索引；随后用户可以通过关键词或自然语言问题检索资料，系统返回相关文档片段。

## 项目成员

- 袁正泽
- 谭张锐
- 邱俊杰
- 黄开轩
- 陈文涛

## 项目目标

本项目重点体现 Rust 在文件处理、数据结构、模块化设计、错误处理和工程协作中的能力。AI 相关能力作为增强模块存在，核心功能不依赖 AI API。

基础目标：

- 扫描本地 Markdown/TXT 文档。
- 解析文档内容并进行文本切片。
- 构建关键词倒排索引。
- 支持命令行搜索。
- 返回相关文档片段、文件路径和相关度分数。
- 提供测试、文档和清晰的成员分工。

增强目标：

- 支持索引缓存。
- 支持搜索结果高亮。
- 支持自然语言问答命令。
- 可选接入 embedding 或 AI API 实现语义检索。

## 推荐目录结构

```text
.
├── README.md
├── Cargo.toml
├── agent.md
├── plan.md
├── 项目协作文档.md
├── 开发日志.md
├── CONTRIBUTING.md
├── docs/
│   ├── 接口设计.md
│   ├── 测试计划.md
│   └── 演示脚本.md
├── examples/
│   └── sample_notes/
└── src/
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

## 计划命令

项目实现后，建议支持以下命令：

```bash
cargo run -- index ./examples/sample_notes
cargo run -- search "Rust 所有权"
cargo run -- search "索引" --top 5
cargo run -- ask "Rust 的所有权是什么？"
```

## 成员分工

- 袁正泽：项目负责人、架构设计、语义增强模块、最终集成。
- 谭张锐：文件扫描模块。
- 邱俊杰：索引构建与搜索排序模块。
- 黄开轩：CLI/TUI 交互与结果展示。
- 陈文涛：文档解析、测试、README 和报告素材。

实际开发中如分工调整，应同步更新本文档、`项目协作文档.md` 和实验报告。

## 开发规范

提交前建议运行：

```bash
cargo fmt
cargo clippy
cargo test
```

代码要求：

- 使用 Rust 作为主要开发语言。
- 合理使用 `Result` 处理错误。
- 避免大量使用 `unwrap()` 和 `expect()`。
- 模块之间通过清晰的数据结构协作。
- 不提交 `target/`、密钥、本地缓存或个人隐私文件。

## 协作文档

- `agent.md`：仓库级 AI Agent 与代码协作规范。
- `plan.md`：两周开发计划。
- `项目协作文档.md`：给组员阅读的协作说明。
- `CONTRIBUTING.md`：Git 分支、提交和合并规范。
- `docs/接口设计.md`：核心数据结构和模块接口。
- `docs/测试计划.md`：单元测试、集成测试和手动测试计划。
- `docs/演示脚本.md`：5 分钟演示视频脚本。

## 课程提交内容

最终提交应包含：

- 项目源码或仓库链接。
- 实验报告。
- 5 分钟以内演示视频。

如果提交源码压缩包，不要包含 `target/` 目录。
