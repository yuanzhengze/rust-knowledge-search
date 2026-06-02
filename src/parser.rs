//! 文档解析模块 —— 读取文件、清洗 Markdown/TXT、提取标题。
//!
//! 负责人:陈文涛(见 `agent.md` §10、`plan.md` Day 3)。
//!
//! 设计要点(见 `docs/接口设计.md` §5):
//! - Markdown 去除基础标记,保留正文。
//! - TXT 直接读取。
//! - 非 UTF-8 时返回 [`AppError::Parse`]。
//!
//! 行号一致性约定:
//! - Markdown 清洗逐行进行,每行清洗后仍占一行(代码围栏行被替换为空行)。
//! - 这样 [`crate::chunker`] 后续按行切分时,`start_line` / `end_line` 与
//!   原始文件行号一一对应。

use std::fs;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::scanner::DocumentMeta;

/// 解析后的完整文档 —— 见 `docs/接口设计.md` §2.2。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// 文档元数据。
    pub meta: DocumentMeta,
    /// 文档标题:Markdown 取首个一级标题,TXT 取首个非空行;否则 `None`。
    pub title: Option<String>,
    /// 清洗后的正文。Markdown 已剥除基础标记,行数与原始文件一致。
    pub content: String,
}

/// 读取并解析单个文档。
///
/// 行为约定:
/// - 后缀为 `.md`(大小写不敏感)走 Markdown 清洗,其它走 TXT 直读。
/// - 文件读取失败返回 [`AppError::Io`]。
/// - 非 UTF-8 内容返回 [`AppError::Parse`],并附带首个非法字节的偏移。
pub fn parse_document(meta: DocumentMeta) -> AppResult<Document> {
    let bytes = fs::read(&meta.path)?;
    let raw = String::from_utf8(bytes).map_err(|err| {
        AppError::Parse(format!(
            "{} is not valid UTF-8 (first invalid byte at offset {})",
            meta.path.display(),
            err.utf8_error().valid_up_to()
        ))
    })?;

    let is_markdown = meta
        .path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md"))
        .unwrap_or(false);

    let (title, content) = if is_markdown {
        (extract_markdown_title(&raw), clean_markdown(&raw))
    } else {
        (extract_txt_title(&raw), raw)
    };

    Ok(Document {
        meta,
        title,
        content,
    })
}

/// 取首个一级标题(`# ...`)作为 Markdown 标题。子级 `##` 等不算。
fn extract_markdown_title(raw: &str) -> Option<String> {
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let title = rest.trim().to_string();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}

/// TXT 标题取首个非空白行(已 trim)。
fn extract_txt_title(raw: &str) -> Option<String> {
    raw.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|s| s.to_string())
}

/// 清洗 Markdown 标记,保留行数与原始文档一致。
///
/// 实现方式:先按行处理代码围栏(把 ```` ``` ```` 行变空行,围栏内文本保留),
/// 再用一组 [`regex`] 替换剥除常见 inline / 行首标记。
fn clean_markdown(raw: &str) -> String {
    let after_fence = strip_code_fences(raw);

    static HTML_COMMENT: OnceLock<Regex> = OnceLock::new();
    static IMAGE: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    static BOLD_STAR: OnceLock<Regex> = OnceLock::new();
    static BOLD_UNDER: OnceLock<Regex> = OnceLock::new();
    static ITALIC_STAR: OnceLock<Regex> = OnceLock::new();
    static ITALIC_UNDER: OnceLock<Regex> = OnceLock::new();
    static CODE_INLINE: OnceLock<Regex> = OnceLock::new();
    static HEADING: OnceLock<Regex> = OnceLock::new();
    static LIST: OnceLock<Regex> = OnceLock::new();
    static BLOCKQUOTE: OnceLock<Regex> = OnceLock::new();

    // (?s) = . 匹配换行,跨行的 HTML 注释也能去掉。
    let html_comment = HTML_COMMENT.get_or_init(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
    let image = IMAGE.get_or_init(|| Regex::new(r"!\[([^\]]*)\]\([^\)]*\)").unwrap());
    let link = LINK.get_or_init(|| Regex::new(r"\[([^\]]+)\]\([^\)]*\)").unwrap());
    let bold_star = BOLD_STAR.get_or_init(|| Regex::new(r"\*\*([^*\n]+)\*\*").unwrap());
    let bold_under = BOLD_UNDER.get_or_init(|| Regex::new(r"__([^_\n]+)__").unwrap());
    let italic_star = ITALIC_STAR.get_or_init(|| Regex::new(r"\*([^*\n]+)\*").unwrap());
    let italic_under = ITALIC_UNDER.get_or_init(|| Regex::new(r"\b_([^_\n]+)_\b").unwrap());
    let code_inline = CODE_INLINE.get_or_init(|| Regex::new(r"`([^`\n]+)`").unwrap());
    // 行首前导用 [ \t] 而非 \s,避免 \s 跨行吞掉 \n 让行数错乱(chunker 行号会错位)。
    let heading = HEADING.get_or_init(|| Regex::new(r"(?m)^[ \t]*#{1,6}[ \t]+").unwrap());
    let list = LIST.get_or_init(|| Regex::new(r"(?m)^[ \t]*[-*+][ \t]+").unwrap());
    let blockquote = BLOCKQUOTE.get_or_init(|| Regex::new(r"(?m)^[ \t]*>[ \t]?").unwrap());

    let mut s = html_comment.replace_all(&after_fence, "").into_owned();
    // image 必须先于 link,否则 ![alt](url) 中的 ! 会被遗留。
    s = image.replace_all(&s, "$1").into_owned();
    s = link.replace_all(&s, "$1").into_owned();
    // bold 必须先于 italic,**x** 否则会被 italic 提前吃掉。
    s = bold_star.replace_all(&s, "$1").into_owned();
    s = bold_under.replace_all(&s, "$1").into_owned();
    s = italic_star.replace_all(&s, "$1").into_owned();
    s = italic_under.replace_all(&s, "$1").into_owned();
    s = code_inline.replace_all(&s, "$1").into_owned();
    s = heading.replace_all(&s, "").into_owned();
    s = list.replace_all(&s, "").into_owned();
    s = blockquote.replace_all(&s, "").into_owned();
    s
}

/// 把 ```` ``` ```` 围栏行替换为空行,围栏内的内容原样保留(便于代码也能被搜)。
/// 关键不变量:输出的行数与输入一致,保证 chunker 行号对齐。
fn strip_code_fences(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_fence = false;
    let mut first = true;
    for line in raw.split('\n') {
        if !first {
            out.push('\n');
        }
        first = false;

        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            // 围栏标记行变空行,保持行数不变。
            continue;
        }
        // 围栏内的文本原样保留(代码也能被索引到)。
        out.push_str(line);
        // 触碰一下避免 unused mut warning(Rust 实际不会触发,但保留语义注释)。
        let _ = in_fence;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 与 scanner 单测一致风格的零依赖临时目录。
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
                "rks-parser-{}-{}-{}-{}",
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

    fn write_file(dir: &Path, rel: &str, content: &str) -> PathBuf {
        let full = dir.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
        full
    }

    fn meta_for(path: &Path) -> DocumentMeta {
        let md = fs::metadata(path).expect("metadata");
        DocumentMeta {
            path: path.to_path_buf(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            file_size: md.len(),
            modified_time: md.modified().ok(),
        }
    }

    #[test]
    fn parse_txt_should_return_document() {
        let dir = TempDir::new("txt");
        let path = write_file(dir.path(), "n.txt", "Hello World\n第二行\n");
        let doc = parse_document(meta_for(&path)).expect("parse txt");
        assert_eq!(doc.title.as_deref(), Some("Hello World"));
        assert_eq!(doc.content, "Hello World\n第二行\n");
    }

    #[test]
    fn parse_txt_title_should_skip_leading_blank_lines() {
        let dir = TempDir::new("txt_blank");
        let path = write_file(dir.path(), "n.txt", "\n\n   \n首行\n第二行\n");
        let doc = parse_document(meta_for(&path)).expect("parse txt");
        assert_eq!(doc.title.as_deref(), Some("首行"));
    }

    #[test]
    fn parse_markdown_should_extract_title() {
        let dir = TempDir::new("md_title");
        let path = write_file(dir.path(), "x.md", "# 标题\n\n正文一行\n");
        let doc = parse_document(meta_for(&path)).expect("parse md");
        assert_eq!(doc.title.as_deref(), Some("标题"));
    }

    #[test]
    fn parse_markdown_should_strip_basic_marks() {
        let dir = TempDir::new("md_strip");
        let raw = "# T\n\n**bold** *italic* `code` [link](http://x) ![alt](img.png)\n";
        let path = write_file(dir.path(), "x.md", raw);
        let doc = parse_document(meta_for(&path)).expect("parse md");

        assert!(doc.content.contains("bold"));
        assert!(doc.content.contains("italic"));
        assert!(doc.content.contains("code"));
        assert!(doc.content.contains("link"));
        assert!(doc.content.contains("alt"));

        assert!(!doc.content.contains("**"));
        assert!(!doc.content.contains("`"));
        assert!(!doc.content.contains("](http"));
        assert!(!doc.content.contains("![")); // image 标记应当被剥
        assert!(!doc.content.contains("# T")); // heading 前导被剥
    }

    #[test]
    fn parse_markdown_should_strip_list_and_blockquote() {
        let dir = TempDir::new("md_list");
        let raw = "# T\n\n- item one\n* item two\n+ item three\n\n> quoted line\n";
        let path = write_file(dir.path(), "x.md", raw);
        let doc = parse_document(meta_for(&path)).expect("parse md");

        for line in doc.content.lines() {
            assert!(
                !line.trim_start().starts_with("- "),
                "list marker leak: {line}"
            );
            assert!(
                !line.trim_start().starts_with("> "),
                "blockquote leak: {line}"
            );
        }
        assert!(doc.content.contains("item one"));
        assert!(doc.content.contains("quoted line"));
    }

    #[test]
    fn parse_markdown_should_preserve_line_count() {
        // 行数稳定是 chunker 行号对齐的前提
        let dir = TempDir::new("md_lines");
        let raw = "# T\n\n## Sub\n\n- a\n- b\n\n```rust\nfn x() {}\n```\n\nend\n";
        let path = write_file(dir.path(), "x.md", raw);
        let doc = parse_document(meta_for(&path)).expect("parse md");

        // String::lines() 不区分末尾是否有换行 — 对原始和清洗后一致比较。
        let raw_lines = raw.lines().count();
        let cleaned_lines = doc.content.lines().count();
        assert_eq!(
            raw_lines, cleaned_lines,
            "Markdown 清洗必须保持行数,否则 chunker 行号会错位"
        );
    }

    #[test]
    fn parse_markdown_should_keep_fenced_code_content() {
        let dir = TempDir::new("md_fence");
        let raw = "# T\n\n```\nfn answer() -> u32 { 42 }\n```\n";
        let path = write_file(dir.path(), "x.md", raw);
        let doc = parse_document(meta_for(&path)).expect("parse md");

        assert!(
            doc.content.contains("fn answer"),
            "围栏内代码应保留,便于索引"
        );
        // 围栏标记行被替换为空行,不应再有 ```
        assert!(!doc.content.contains("```"));
    }

    #[test]
    fn parse_document_should_handle_empty_file() {
        let dir = TempDir::new("empty");
        let path = write_file(dir.path(), "e.md", "");
        let doc = parse_document(meta_for(&path)).expect("parse empty");
        assert_eq!(doc.title, None);
        assert_eq!(doc.content, "");
    }

    #[test]
    fn parse_document_should_reject_non_utf8() {
        let dir = TempDir::new("bad_utf8");
        let path = dir.path().join("bad.txt");
        fs::write(&path, [0xff, 0xfe, 0xfd, b'a', b'b']).unwrap();
        let err = parse_document(meta_for(&path)).expect_err("non-utf8 must error");
        assert!(
            matches!(err, AppError::Parse(_)),
            "non-utf8 should map to AppError::Parse, got {err:?}"
        );
    }

    #[test]
    fn parse_real_sample_md() {
        // 端到端验证:examples/sample_notes/rust_ownership.md
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = PathBuf::from(manifest).join("examples/sample_notes/rust_ownership.md");
        let doc = parse_document(meta_for(&path)).expect("parse sample md");

        assert_eq!(doc.title.as_deref(), Some("Rust 所有权笔记"));
        assert!(doc.content.contains("所有权"));
        // 标题前导和列表前导都应被剥
        assert!(!doc.content.contains("# Rust"));
        assert!(!doc.content.contains("## Move"));
        // 行内反引号去掉,但内容保留
        assert!(!doc.content.contains("`Copy`"));
        assert!(doc.content.contains("Copy"));
    }

    #[test]
    fn parse_real_sample_txt() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = PathBuf::from(manifest).join("examples/sample_notes/project_intro.txt");
        let doc = parse_document(meta_for(&path)).expect("parse sample txt");
        assert_eq!(doc.title.as_deref(), Some("项目简介"));
        // TXT 应当原样保留
        assert!(doc.content.contains("基于 Rust"));
    }
}
