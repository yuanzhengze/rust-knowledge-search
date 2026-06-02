//! 文件扫描模块 —— 递归遍历目录、过滤可处理文件、读取元数据。
//!
//! 负责人:谭张锐(见 `agent.md` §10、`plan.md` Day 2)。
//!
//! 设计要点(见 `docs/接口设计.md` §4):
//! - 仅支持 `.md` / `.txt`(扩展名大小写不敏感)。
//! - 忽略隐藏文件和隐藏目录(以 `.` 开头),以及 `target/`。
//! - 不在此模块读取文件正文,正文由 `parser` 模块负责。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use walkdir::{DirEntry, WalkDir};

use crate::error::{AppError, AppResult};

/// 文件元数据 —— 跨模块的稳定数据结构,字段见 `docs/接口设计.md` §2.1。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    /// 文件完整路径。
    pub path: PathBuf,
    /// 文件名(不含目录)。
    pub file_name: String,
    /// 文件大小(字节)。
    pub file_size: u64,
    /// 最后修改时间;读取失败时为 `None`。
    pub modified_time: Option<SystemTime>,
}

/// 递归扫描指定目录,返回所有支持的文件元数据。
///
/// 行为约定:
/// - `root` 不存在或不是目录时返回 [`AppError::InvalidPath`]。
/// - 空目录返回空 `Vec`,不报错。
/// - 隐藏文件、隐藏目录、`target/` 会被剪枝。
/// - 仅保留扩展名为 `.md` / `.txt`(大小写不敏感)的常规文件。
pub fn scan_documents(root: &Path) -> AppResult<Vec<DocumentMeta>> {
    if !root.exists() {
        return Err(AppError::InvalidPath(format!(
            "path not found: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(AppError::InvalidPath(format!(
            "path is not a directory: {}",
            root.display()
        )));
    }

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e) && !is_excluded_dir(e));

    let mut docs = Vec::new();
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // 把 walkdir::Error 中的 IO 错误透传给上层,目录权限问题不应静默吞掉。
                if let Some(io_err) = err.into_io_error() {
                    return Err(AppError::Io(io_err));
                }
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }
        if !is_supported_extension(entry.path()) {
            continue;
        }

        let metadata = entry.metadata().map_err(|err| {
            err.into_io_error()
                .map(AppError::Io)
                .unwrap_or_else(|| AppError::InvalidPath("metadata read failed".into()))
        })?;

        docs.push(DocumentMeta {
            path: entry.path().to_path_buf(),
            file_name: entry.file_name().to_string_lossy().into_owned(),
            file_size: metadata.len(),
            modified_time: metadata.modified().ok(),
        });
    }

    Ok(docs)
}

/// 判断条目是否为隐藏文件 / 隐藏目录(以 `.` 开头)。
///
/// 仅对 `depth > 0` 的子项生效,避免在 `root` 自身就是隐藏目录(如 `./.notes`)
/// 时被整体剪掉。
fn is_hidden(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry
            .file_name()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
}

/// 排除常见构建产物目录。当前只跳过 `target/`,与 `.gitignore` 保持一致;
/// 后续如需扩展(如 `node_modules/`),再在此处集中维护。
fn is_excluded_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() || entry.depth() == 0 {
        return false;
    }
    matches!(entry.file_name().to_str(), Some("target"))
}

/// 仅保留扩展名为 `md` / `txt` 的常规文件,扩展名比较忽略大小写。
fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            lower == "md" || lower == "txt"
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 简易临时目录:不引入 `tempfile` 依赖,避免越界改 `Cargo.toml`。
    /// 通过进程 ID + 时间戳 + 全局计数器保证多线程并行测试时目录不冲突。
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
                "rks-scanner-{}-{}-{}-{}",
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

    fn write(dir: &Path, rel: &str, content: &str) {
        let full = dir.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(full, content).expect("write file");
    }

    fn names(metas: &[DocumentMeta]) -> Vec<String> {
        let mut v: Vec<String> = metas.iter().map(|m| m.file_name.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn scan_documents_should_find_supported_files() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let sample = PathBuf::from(manifest).join("examples/sample_notes");

        let metas = scan_documents(&sample).expect("scan sample_notes");
        let got = names(&metas);

        assert_eq!(
            got,
            vec![
                "error_handling.txt".to_string(),
                "project_intro.txt".to_string(),
                "rust_ownership.md".to_string(),
                "search_system.md".to_string(),
            ],
            "examples/sample_notes 中应当只识别 4 个 .md/.txt"
        );
        for meta in &metas {
            assert!(meta.file_size > 0, "示例文档应当有内容");
            assert!(meta.path.is_absolute() || meta.path.starts_with(manifest));
        }
    }

    #[test]
    fn scan_documents_should_ignore_unsupported_files() {
        let dir = TempDir::new("unsupported");
        write(dir.path(), "a.md", "# a");
        write(dir.path(), "b.txt", "b");
        write(dir.path(), "c.pdf", "");
        write(dir.path(), "d.log", "");
        write(dir.path(), "no_ext", "");
        // 大小写应当也被识别。
        write(dir.path(), "e.MD", "# e");
        write(dir.path(), "f.TxT", "f");

        let metas = scan_documents(dir.path()).expect("scan unsupported");
        assert_eq!(
            names(&metas),
            vec![
                "a.md".to_string(),
                "b.txt".to_string(),
                "e.MD".to_string(),
                "f.TxT".to_string(),
            ]
        );
    }

    #[test]
    fn scan_documents_should_return_error_for_missing_path() {
        let dir = TempDir::new("missing");
        let missing = dir.path().join("does_not_exist");

        let err = scan_documents(&missing).expect_err("missing path must error");
        assert!(
            matches!(err, AppError::InvalidPath(_)),
            "missing path should map to InvalidPath, got {err:?}"
        );
    }

    #[test]
    fn scan_documents_should_return_error_when_path_is_file() {
        let dir = TempDir::new("file_input");
        write(dir.path(), "only.md", "# only");
        let file_path = dir.path().join("only.md");

        let err = scan_documents(&file_path).expect_err("file path must error");
        assert!(
            matches!(err, AppError::InvalidPath(_)),
            "file path should map to InvalidPath, got {err:?}"
        );
    }

    #[test]
    fn scan_documents_should_return_empty_for_empty_dir() {
        let dir = TempDir::new("empty");
        let metas = scan_documents(dir.path()).expect("scan empty");
        assert!(metas.is_empty());
    }

    #[test]
    fn scan_documents_should_skip_hidden_entries() {
        let dir = TempDir::new("hidden");
        write(dir.path(), "visible.md", "# v");
        write(dir.path(), ".hidden.md", "# h");
        write(dir.path(), ".hidden_dir/inside.md", "# i");
        write(dir.path(), "sub/deep.txt", "deep");

        let metas = scan_documents(dir.path()).expect("scan hidden");
        assert_eq!(
            names(&metas),
            vec!["deep.txt".to_string(), "visible.md".to_string()]
        );
    }

    #[test]
    fn scan_documents_should_skip_target_dir() {
        let dir = TempDir::new("target");
        write(dir.path(), "doc.md", "# doc");
        write(dir.path(), "target/built.md", "# built");
        write(dir.path(), "src/notes.txt", "n");

        let metas = scan_documents(dir.path()).expect("scan target");
        assert_eq!(
            names(&metas),
            vec!["doc.md".to_string(), "notes.txt".to_string()]
        );
    }

    #[test]
    fn scan_documents_should_collect_metadata_fields() {
        let dir = TempDir::new("metadata");
        write(dir.path(), "n.md", "# hello");

        let metas = scan_documents(dir.path()).expect("scan metadata");
        assert_eq!(metas.len(), 1);
        let only = &metas[0];
        assert_eq!(only.file_name, "n.md");
        assert_eq!(only.file_size, "# hello".len() as u64);
        assert!(only.modified_time.is_some(), "modified_time 应当能读到");
        assert!(only.path.ends_with("n.md"));
    }
}
