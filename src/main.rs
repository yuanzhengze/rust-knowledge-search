//! 二进制入口 —— 仅做参数分发,业务逻辑全部在 `cli::run` 内组合。
//!
//! 把 `AppResult<()>` 直接返回,Rust runtime 会自动用 `Debug` 打印 `AppError`,
//! 无需在此处再写一层 match。

use rust_knowledge_search::AppResult;

fn main() -> AppResult<()> {
    rust_knowledge_search::cli::run()
}
