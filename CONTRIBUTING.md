# 贡献与协作规范

本文档用于说明本项目的 Git 协作方式、提交规范和合并前检查要求。

## 1. 分支规范

推荐使用以下分支：

- `main`：稳定主分支，只合并已经测试过的功能。
- `feature/scanner`：文件扫描模块。
- `feature/parser`：文档解析模块。
- `feature/indexer`：索引构建模块。
- `feature/search`：搜索排序模块。
- `feature/cli`：命令行交互模块。
- `feature/semantic`：语义增强模块。
- `feature/docs`：文档、报告和演示材料。

每位成员优先在自己负责的功能分支上开发，避免直接向 `main` 提交未测试代码。

## 2. 提交信息规范

提交信息建议使用：

```text
类型: 简短说明
```

常用类型：

- `feat`：新增功能。
- `fix`：修复问题。
- `test`：新增或修改测试。
- `docs`：修改文档。
- `refactor`：重构代码。
- `chore`：依赖、配置、杂项调整。

示例：

```text
feat: add directory scanner
feat: implement document chunker
fix: handle empty search query
test: add parser unit tests
docs: update usage guide
```

## 3. 合并前检查

合并前请尽量完成：

```bash
cargo fmt
cargo clippy
cargo test
```

如果某项暂时无法通过，应在提交说明或群同步中说明原因。

## 4. 代码提交要求

- 一次提交只做一个相对完整的主题。
- 不提交 `target/` 目录。
- 不提交 `.env`、API Key、密钥或个人隐私文件。
- 不为了通过编译删除测试或绕过核心逻辑。
- 修改公共数据结构或函数签名前，应通知相关模块负责人。
- 新增核心功能时应补充测试或说明测试方式。

## 5. 每日同步要求

建议每天 21:00 前在群里同步：

- 昨天完成了什么。
- 今天准备做什么。
- 当前遇到的问题。

如果当天没有提交代码，也需要说明当前进度。

## 6. 文档同步要求

以下内容发生变化时，需要更新相关文档：

- 成员分工变化。
- 项目命令变化。
- 核心数据结构变化。
- 模块职责变化。
- 测试方式变化。
- 演示流程变化。

优先更新：

- `README.md`
- `项目协作文档.md`
- `docs/接口设计.md`
- `docs/测试计划.md`
