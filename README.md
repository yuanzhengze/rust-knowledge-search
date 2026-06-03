# 基于 Rust 的本地知识库语义搜索系统

[![Tests](https://img.shields.io/badge/tests-96%20unit%20%2B%204%20integration-brightgreen)]() [![Rust](https://img.shields.io/badge/rust-2021-orange)]() [![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)]()

本项目是 Rust 程序设计课程期末大作业。系统是一个**纯本地**的命令行工具：扫描指定目录下的 Markdown / TXT 文档，构建关键词倒排索引，支持关键词检索与自然语言问答。AI 仅作为可选增强，**没有 API Key、没有网络也能完整使用基础功能**。

## 项目状态

✅ **MVP 完成 + Day 9 / Day 10-11 增强已落地** —— 三条命令 `index` / `search` / `ask` 全部可用，96 个单元测试 + 4 个集成测试全部通过。`ask` 命令在配置 `AI_API_KEY` 时调用 OpenAI 兼容的 Chat Completions API 给真实回答，未配置时降级到候选片段展示。

具体能力：

- 递归扫描目录，识别 `.md` / `.txt` 文件，跳过隐藏文件、`target/` 等噪声目录
- Markdown 标题提取与基础标记清洗（标题 / 列表 / 引用 / 链接 / 行内代码 / 加粗斜体），保持行号对齐源文件
- 文档切片（按字符上限 + 行号边界），生成稳定 ID `<path>#L<start>-L<end>`
- 中英文混合分词的倒排索引（CJK 按字符 unigram，ASCII 按字母数字累积），词频评分 + Top K 排序
- JSON 缓存原子写入（临时文件 + rename），从环境变量读取缓存路径
- `ask` 命令在 AI 不可用时自动降级展示候选片段

## 快速上手

### 1. 编译

```bash
cargo build --release
```

或开发模式：

```bash
cargo build
```

### 2. 建立索引

```bash
cargo run -- index ./examples/sample_notes
```

默认会在当前目录生成 `.knowledge_index.json`（已在 `.gitignore` 中）。可通过环境变量自定义：

```bash
KNOWLEDGE_INDEX_PATH=/tmp/my_index.json cargo run -- index ./examples/sample_notes
```

输出示例：

```text
扫描到 4 个文档
切分得到 9 个片段
索引共 327 个关键词
索引已保存到 ./.knowledge_index.json
```

### 3. 关键词搜索

```bash
cargo run -- search "Rust 所有权"
cargo run -- search "索引" --top 3
```

输出示例（节选）：

```text
找到 3 条结果:

[1] ./examples/sample_notes/rust_ownership.md#L1-L14  分数 39.00
    匹配关键词: rust, 所, 有, 权
    Rust 所有权笔记...

[2] ./examples/sample_notes/rust_ownership.md#L15-L23  分数 16.00
    匹配关键词: rust, 所, 有, 权
    当一个值被赋给另一个变量时...
```

### 4. 自然语言问答

```bash
cargo run -- ask "Rust 的所有权是什么?"
```

未配置 AI 后端时（默认情况），自动降级展示相关片段：

```text
[提示] AI 引擎不可用 (semantic engine disabled)。以下是检索到的相关片段:

[1] ./examples/sample_notes/rust_ownership.md#L1-L14  分数 39.00
    Rust 所有权笔记...
```

配置任意 OpenAI 兼容的 chat 后端（OpenAI / DeepSeek / 智谱 / 月之暗面 / SiliconFlow / OpenRouter 等）后，自动启用真实 AI 回答：

```bash
export AI_API_KEY=sk-...
export AI_BASE_URL=https://api.deepseek.com/v1   # 可选，默认 https://api.openai.com/v1
export AI_CHAT_MODEL=deepseek-chat                # 可选，默认 gpt-4o-mini
cargo run -- ask "Rust 的所有权是什么?"
```

输出会变为：

```text
AI 回答:
Rust 的所有权机制让每个值都有唯一所有者，作用域结束时自动释放...

引用片段:

[1] ./examples/sample_notes/rust_ownership.md#L1-L14  分数 39.00
    ...
```

引擎实现见 `src/semantic.rs::ChatEngine`，使用 `reqwest` 同步 HTTP 调用 `{AI_BASE_URL}/chat/completions` 端点。任何错误（无 API_KEY、网络断、API 返回 4xx/5xx、JSON 解析失败）都映射为 `AppError::Semantic`，cli 层自动降级为候选片段展示，**永远不会让 ask 命令崩溃**。

## 命令一览

| 命令 | 用途 | 关键参数 |
|---|---|---|
| `index <path>` | 扫描目录、构建索引并写入缓存 | 必填:知识库根目录 |
| `search <query> [--top N]` | 在已建索引上做关键词检索 | `--top` 默认 5 |
| `ask <question>` | 自然语言问答(AI 降级到候选片段) | — |

任何命令加 `--help` 均可查看帮助：

```bash
cargo run -- --help
cargo run -- search --help
```

## 环境变量

| 变量 | 用途 | 缺省 |
|---|---|---|
| `KNOWLEDGE_INDEX_PATH` | 索引缓存文件路径 | `.knowledge_index.json` |
| `AI_API_KEY` | OpenAI 兼容 chat API 的密钥 | 未设置时 ask 走降级路径 |
| `AI_BASE_URL` | OpenAI 兼容 chat API 的 base URL | `https://api.openai.com/v1` |
| `AI_CHAT_MODEL` | 调用的 chat 模型 ID | `gpt-4o-mini` |
| `AI_EMBEDDING_MODEL` | (预留)embedding 模型 ID | 未读取 |

参考 `.env.example`。`ask` 命令在配置 `AI_API_KEY` 时启用 `ChatEngine` 调用真实 LLM；未配置时落到 `NoopEngine` 走候选片段展示。

## 目录结构

```text
.
├── Cargo.toml / Cargo.lock      # 二进制 crate 配置(Cargo.lock 入库)
├── README.md                    # 本文件
├── agent.md                     # 仓库级 AI Agent 协作规范
├── plan.md                      # 两周开发计划
├── 项目协作文档.md               # 给组员的协作说明
├── 开发日志.md                   # 每日开发记录
├── CONTRIBUTING.md              # Git / commit / 合并规范
├── .env.example                 # 环境变量样例
├── docs/
│   ├── 接口设计.md               # 核心数据结构与模块接口
│   ├── 测试计划.md               # 单元测试 / 集成测试计划
│   ├── 演示脚本.md               # 5 分钟视频脚本
│   ├── 实验报告素材.md           # 给 docx 模板的报告素材
│   └── 文件管理说明.md           # 目录与文件归类约定
├── examples/
│   └── sample_notes/            # 测试与演示用的示例知识库
└── src/
    ├── main.rs                  # 二进制入口
    ├── lib.rs                   # 库入口与 re-export
    ├── cli.rs                   # clap 子命令分发与渲染(黄开轩)
    ├── scanner.rs               # 目录扫描(谭张锐)
    ├── parser.rs                # 文档解析(陈文涛)
    ├── chunker.rs               # 文档切片(陈文涛)
    ├── indexer.rs               # 倒排索引(邱俊杰)
    ├── search.rs                # 搜索排序(邱俊杰)
    ├── storage.rs               # 索引缓存(袁正泽)
    ├── semantic.rs              # 可选语义增强(谭张锐, Day 10-11 移交)
    └── error.rs                 # 统一错误类型 AppError / AppResult
```

## 数据流

```text
CLI(cli.rs)
  ↓
scanner    递归遍历,过滤 .md/.txt,只读元数据
  ↓
parser     UTF-8 校验,Markdown 清洗,提取标题
  ↓
chunker    按 max_chars 切片,生成稳定 ID 与行号
  ↓
indexer    分词,构建 term → chunk_ids 映射与词频
  ↓                                                       ↑
search     评分排序,返回 Top K SearchResult              storage(JSON 缓存)
  ↓
semantic   可选语义重排或 AI 问答(NoopEngine 降级)
  ↓
CLI 渲染输出
```

## 测试与质量门

```bash
cargo fmt            # 代码格式化
cargo clippy --all-targets -- -D warnings  # 严格静态检查
cargo test           # 单测 + 集成测试
cargo test scanner   # 仅跑某个模块的测试
```

测试规模:

- **scanner** 8 个 / **parser** 11 个 / **chunker** 11 个 / **indexer** 9 个 / **search** 10 个 / **storage** 7 个 / **semantic** 11 个 / **cli** 29 个 = **96 单元测试**
- **集成测试** 4 个:完整索引流程、缓存复用、ask 降级路径、ANSI 高亮验证

详见 `docs/测试计划.md`。

## 成员分工

| 成员 | 主要模块 | 交付物 |
|---|---|---|
| **袁正泽** | 项目负责人、架构设计、`storage.rs`、`error.rs` | 整体架构、缓存原子写入、统一错误类型 |
| **谭张锐** | `scanner.rs`、`semantic.rs`(Day 10-11 移交) | 目录扫描与元数据收集、语义增强 + AI 接入 |
| **邱俊杰** | `indexer.rs`、`search.rs` | 倒排索引、词频排序、Top K |
| **黄开轩** | `cli.rs`、`main.rs` | 命令行交互、结果渲染、ANSI 高亮、集成测试启用 |
| **陈文涛** | `parser.rs`、`chunker.rs`、文档与报告素材 | 解析与切片、README、演示脚本、报告素材 |

具体提交记录见 Git log 与 `开发日志.md`。

## Rust 特性使用清单

- **所有权与借用**:`&Path` / `&str` 在调用链上传递,避免不必要的克隆;`InvertedIndex` 内部 `HashMap` 持有 `String` 键拥有所有权
- **`Result` 与自定义错误**:`AppError`(thiserror 派生)+ `AppResult<T>` 贯穿全模块;`#[from]` 自动从 `io::Error` / `serde_json::Error` 转换
- **`struct` / `enum`**:`DocumentMeta` / `Document` / `DocumentChunk` / `SearchResult` 跨模块数据契约;`AppError` / `Command`(clap subcommand)用 enum
- **`trait`**:`SemanticEngine` 定义可替换的语义后端,`NoopEngine` 是其默认占位实现
- **模块化**:每个职责一个 `mod`,模块间通过公共数据结构(`docs/接口设计.md` §2)耦合,内部细节用 `pub(crate)` 隐藏(如 `indexer::tokenize`)
- **集合**:`HashMap<String, Vec<String>>` 实现倒排;`HashMap<String, HashMap<String, u32>>` 记录词频;`HashSet` / `BTreeSet` 做去重
- **泛型**:`AppResult<T>` 是 `Result<T, AppError>` 的别名,所有可失败函数复用
- **派生宏**:`#[derive(Debug, Clone, Serialize, Deserialize)]` 大量使用,让数据结构在 storage 缓存与单测断言中复用
- **零依赖临时目录**:测试中实现 `TempDir` RAII 结构,通过 `Drop` 自动清理,不引入 `tempfile` 依赖

## 创新点

1. **中文友好的零依赖分词**:CJK Unified Ideographs 按字符 unigram,ASCII 按字母数字累积,既不需要 jieba 这类大字典依赖,又能支持中英文混合查询
2. **行号对齐的 Markdown 清洗**:parser 清洗时严格保持行数不变(代码围栏行替换为空行),让 chunker 报告的 `start_line` / `end_line` 直接对应原始文件行号,后续可以做精确高亮回溯
3. **POSIX 原子缓存写入**:`storage::save_index` 写到同目录临时文件再 rename,中断不会留半写文件,JSON 损坏可以明确区分"语义级别索引未就绪"(`AppError::Index`)与"序列化失败"(`AppError::Serde`)
4. **可插拔 + 可降级的 AI 集成**:`SemanticEngine` 是 trait,`ChatEngine` 调用 OpenAI 兼容协议(一份代码接入 OpenAI/DeepSeek/智谱/Kimi 等所有兼容供应商),`NoopEngine` 在未配置 API_KEY 或网络失败时托底。任何错误都映射为 `AppError::Semantic`,cli 层用 `match` 自动降级 —— **整个项目可以在没有任何外部网络的环境中完整运行,接入 AI 后又能立刻给出真实回答**
5. **ANSI 着色与智能 snippet**(Day 9 黄开轩):`--color <auto|always|never>` 全局参数遵守 `NO_COLOR` 事实标准与 `IsTerminal` TTY 检测,管道与 CI 自动关闭着色;snippet 围绕首个匹配 token 取上下文(前后各 60 字符),按 char 而非 byte 截断,UTF-8 安全

## 课程提交内容

- 项目源码或仓库链接(注意排除 `target/` 和 `.knowledge_index.json`)
- 实验报告(基于 `docs/实验报告素材.md` 填入 docx 模板)
- 5 分钟以内演示视频(参考 `docs/演示脚本.md`)
- 成员分工说明(本 README + `项目协作文档.md`)

## 开发规范

提交前请运行:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

代码要求(详见 `agent.md`):

- 主要使用 Rust 实现核心逻辑
- 用 `Result` 处理错误,避免 `unwrap()` / `expect()`(测试除外)
- 模块之间通过清晰的数据结构协作,不跨模块写公共逻辑
- 不提交 `target/`、`.env`、`.knowledge_index.json` 或 `private/` / `secrets/` 目录

## 协作文档导航

- `agent.md` —— 仓库级 AI Agent 与代码协作规范
- `plan.md` —— 两周开发计划与里程碑
- `项目协作文档.md` —— 给组员阅读的协作说明
- `CONTRIBUTING.md` —— Git 分支、commit 与合并规范
- `开发日志.md` —— 按日期记录的开发过程
- `docs/接口设计.md` —— 跨模块数据结构与函数签名契约
- `docs/测试计划.md` —— 单元 / 集成 / 手动测试覆盖
- `docs/演示脚本.md` —— 5 分钟视频脚本
- `docs/实验报告素材.md` —— 实验报告填空素材
- `docs/文件管理说明.md` —— 目录归类与命名约定

## License

MIT OR Apache-2.0(见 `Cargo.toml`)。
