//! 索引缓存模块 —— 把 [`InvertedIndex`] 与 [`DocumentChunk`] 集合持久化到 JSON。
//!
//! 负责人:袁正泽(见 `agent.md` §10、`plan.md` Day 8)。
//!
//! 设计要点(见 `docs/接口设计.md` §9):
//! - 使用 [`serde_json`] 做格式,易读、便于调试。
//! - **原子写入**:写到同目录 `<path>.tmp.<pid>.<nanos>` 临时文件后用
//!   [`fs::rename`] 替换目标。同设备 rename 是原子的,避免被中断后留下半写文件。
//! - 缓存文件不存在 → [`AppError::Index`](crate::error::AppError::Index)
//!   (按接口约定,这是"语义级别的索引未就绪",不是底层 IO 错误)。
//! - JSON 损坏 → [`AppError::Serde`](crate::error::AppError::Serde),
//!   通过 `#[from]` 自动映射,调用方(`cli`)负责降级提示。
//! - 默认缓存路径由环境变量 `KNOWLEDGE_INDEX_PATH` 决定(见 `.env.example`),
//!   解析逻辑在 `cli` 层,本模块只接收 `&Path` 参数。

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::chunker::DocumentChunk;
use crate::error::{AppError, AppResult};
use crate::indexer::InvertedIndex;

/// 缓存文件的物理结构 —— `index` 命令写入,`search`/`ask` 命令读取。
///
/// 字段公开,方便 storage 测试和后续工具直接反序列化(例如 `dump` 调试)。
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub index: InvertedIndex,
    pub chunks: Vec<DocumentChunk>,
}

/// 把索引和 chunk 集合保存到 `path` 指定的 JSON 文件。
///
/// 行为约定:
/// - 父目录不存在时自动 `create_dir_all`。
/// - 通过 "写临时文件 + rename" 实现原子替换,中途失败不会留下半写的目标文件。
/// - 失败时尽力清理临时文件,但清理错误不掩盖主错误。
pub fn save_index(path: &Path, index: &InvertedIndex, chunks: &[DocumentChunk]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    let snapshot = IndexSnapshot {
        index: index.clone(),
        chunks: chunks.to_vec(),
    };

    let tmp = make_tmp_path(path);

    // 显式作用域确保 BufWriter / File 在 rename 前释放、flush。
    let write_result = (|| -> AppResult<()> {
        let file = File::create(&tmp)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &snapshot)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        writer
            .into_inner()
            .map_err(|e| e.into_error())?
            .sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_result {
        // 写入失败:清理临时文件后冒泡原始错误。
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }

    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(AppError::Io(err));
    }

    Ok(())
}

/// 从 `path` 加载索引快照。
///
/// - 文件不存在返回 [`AppError::Index`](crate::error::AppError::Index)。
/// - JSON 反序列化失败由 [`serde_json::Error`] 自动映射到 [`AppError::Serde`]。
pub fn load_index(path: &Path) -> AppResult<(InvertedIndex, Vec<DocumentChunk>)> {
    if !path.exists() {
        return Err(AppError::Index(format!(
            "index cache not found: {}",
            path.display()
        )));
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let snapshot: IndexSnapshot = serde_json::from_reader(reader)?;
    Ok((snapshot.index, snapshot.chunks))
}

/// 生成同目录下的唯一临时文件名,降低多进程并发写时碰撞概率。
fn make_tmp_path(target: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let suffix = format!(".tmp.{}.{}", std::process::id(), nanos);
    let mut buf = target.as_os_str().to_owned();
    buf.push(&suffix);
    PathBuf::from(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 与 scanner / parser 一致风格的零依赖临时目录。
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
                "rks-storage-{}-{}-{}-{}",
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

    fn sample_chunk(id: &str, content: &str) -> DocumentChunk {
        DocumentChunk {
            id: id.to_string(),
            file_path: PathBuf::from(format!("/virtual/{id}.md")),
            title: Some("T".into()),
            content: content.to_string(),
            start_line: 1,
            end_line: 1,
        }
    }

    fn sample_data() -> (InvertedIndex, Vec<DocumentChunk>) {
        let chunks = vec![
            sample_chunk("c1", "Rust 所有权 是核心概念"),
            sample_chunk("c2", "Rust 错误处理 Result"),
        ];
        let index = InvertedIndex::build(&chunks);
        (index, chunks)
    }

    #[test]
    fn save_and_load_index_should_preserve_data() {
        let dir = TempDir::new("roundtrip");
        let cache = dir.path().join("idx.json");
        let (index, chunks) = sample_data();

        save_index(&cache, &index, &chunks).expect("save ok");
        assert!(cache.exists(), "缓存文件应当存在");

        let (loaded_index, loaded_chunks) = load_index(&cache).expect("load ok");
        assert_eq!(loaded_chunks.len(), chunks.len());
        // 关键查询能力得保留:lookup 和原本一致。
        let mut hits = loaded_index.lookup("Rust");
        hits.sort();
        let mut expected = index.lookup("Rust");
        expected.sort();
        assert_eq!(hits, expected);
        assert_eq!(loaded_index.lookup("权"), index.lookup("权"));
    }

    #[test]
    fn load_index_should_fail_for_missing_file() {
        let dir = TempDir::new("missing");
        let cache = dir.path().join("nope.json");

        let err = load_index(&cache).expect_err("missing must error");
        assert!(
            matches!(err, AppError::Index(_)),
            "missing cache should map to AppError::Index, got {err:?}"
        );
    }

    #[test]
    fn load_index_should_fail_for_invalid_json() {
        let dir = TempDir::new("invalid");
        let cache = dir.path().join("broken.json");
        fs::write(&cache, b"this is not json {[").unwrap();

        let err = load_index(&cache).expect_err("invalid json must error");
        assert!(
            matches!(err, AppError::Serde(_)),
            "invalid json should map to AppError::Serde, got {err:?}"
        );
    }

    #[test]
    fn save_should_create_parent_directory() {
        let dir = TempDir::new("nested");
        let cache = dir.path().join("a/b/c/idx.json");
        let (index, chunks) = sample_data();

        save_index(&cache, &index, &chunks).expect("save into nested dir");
        assert!(cache.exists());
    }

    #[test]
    fn save_should_overwrite_existing_cache_atomically() {
        let dir = TempDir::new("overwrite");
        let cache = dir.path().join("idx.json");

        // 先写一份旧数据。
        let (idx_v1, ch_v1) = sample_data();
        save_index(&cache, &idx_v1, &ch_v1).expect("v1");

        // 再写一份新数据(只有一个 chunk)。
        let new_chunks = vec![sample_chunk("only", "Python 装饰器")];
        let new_index = InvertedIndex::build(&new_chunks);
        save_index(&cache, &new_index, &new_chunks).expect("v2");

        let (loaded, loaded_chunks) = load_index(&cache).expect("load");
        assert_eq!(loaded_chunks.len(), 1);
        assert_eq!(loaded_chunks[0].id, "only");
        assert_eq!(loaded.lookup("装"), vec!["only".to_string()]);
        // 旧数据应当完全被替换,Rust 关键字 lookup 为空。
        assert!(loaded.lookup("Rust").is_empty());
    }

    #[test]
    fn save_should_not_leave_tmp_file_on_success() {
        let dir = TempDir::new("no_tmp_leak");
        let cache = dir.path().join("idx.json");
        let (index, chunks) = sample_data();

        save_index(&cache, &index, &chunks).expect("save ok");

        // 同目录内不应残留 .tmp.* 文件。
        let tmp_residue: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.contains(".tmp."))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            tmp_residue.is_empty(),
            "成功的 save 不应残留临时文件: {tmp_residue:?}"
        );
    }

    #[test]
    fn save_then_load_then_search_should_match() {
        // 端到端:保存 → 加载 → 用加载后的索引做一次 search。
        use crate::search::search;

        let dir = TempDir::new("e2e");
        let cache = dir.path().join("idx.json");
        let (index, chunks) = sample_data();
        save_index(&cache, &index, &chunks).expect("save ok");

        let (loaded_index, loaded_chunks) = load_index(&cache).expect("load ok");
        let results =
            search(&loaded_index, &loaded_chunks, "Rust", 5).expect("search on loaded ok");
        assert!(!results.is_empty(), "Rust 应当至少命中一条");
        assert!(results.iter().all(|r| r.score > 0.0));
    }

    // -----------------------------------------------------------------------
    // Day 12 边界测试: 真实文件系统场景
    // -----------------------------------------------------------------------

    #[test]
    fn save_should_handle_path_with_spaces() {
        let dir = TempDir::new("path_with_spaces");
        let cache = dir.path().join("my cache file.json");
        let (index, chunks) = sample_data();

        save_index(&cache, &index, &chunks).expect("save to spaced path");
        let (loaded, _) = load_index(&cache).expect("load from spaced path");
        assert!(!loaded.lookup("Rust").is_empty());
    }

    #[test]
    fn save_should_handle_path_with_chinese() {
        let dir = TempDir::new("zh_path");
        let cache = dir.path().join("我的索引.json");
        let (index, chunks) = sample_data();

        save_index(&cache, &index, &chunks).expect("save to chinese path");
        let (loaded, _) = load_index(&cache).expect("load from chinese path");
        assert!(!loaded.lookup("Rust").is_empty());
    }

    #[test]
    fn save_should_handle_path_with_emoji() {
        let dir = TempDir::new("emoji_path");
        let cache = dir.path().join("✨idx.json");
        let (index, chunks) = sample_data();

        save_index(&cache, &index, &chunks).expect("save to emoji path");
        let (loaded, _) = load_index(&cache).expect("load from emoji path");
        assert!(!loaded.lookup("Rust").is_empty());
    }

    /// 在只读父目录里 save 应当返回 IO 错误而不是 panic。仅 Unix 可控制权限。
    #[cfg(unix)]
    #[test]
    fn save_should_fail_for_readonly_parent_directory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("readonly_parent");
        let parent = dir.path().join("ro_dir");
        fs::create_dir_all(&parent).unwrap();
        // 0o555 = r-x r-x r-x: 可以列出 / cd, 但不能在里面创建文件
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();

        let cache = parent.join("idx.json");
        let (index, chunks) = sample_data();
        let result = save_index(&cache, &index, &chunks);

        // 恢复权限以便 TempDir Drop 时能清理
        let _ = fs::set_permissions(&parent, fs::Permissions::from_mode(0o755));

        // 大部分 Unix 系统下普通用户在只读目录创建文件会得到 EACCES → AppError::Io
        match result {
            Err(AppError::Io(_)) => {} // 期望路径
            Ok(()) => {
                // 极少数情况(root 用户、特殊文件系统如 /tmp 配置宽松等)可能成功,
                // 不强行失败,但要求文件实际存在
                assert!(cache.exists(), "如果 save 返回 Ok,缓存文件应当真实存在");
            }
            Err(other) => panic!("expected Io error, got {other:?}"),
        }
    }
}
