//! 文档切片模块 —— 把 [`Document`] 拆分为多个 [`DocumentChunk`]。
//!
//! 负责人:陈文涛(见 `agent.md` §10、`plan.md` Day 4)。
//!
//! 设计要点(见 `docs/接口设计.md` §6):
//! - chunk 大小由 `max_chars` 控制,过短的文档至少返回一个 chunk。
//! - chunk ID 形式 `<file_path>#L<start>-L<end>`,稳定且唯一。
//! - 行号 1-based,与编辑器和大多数工具一致。
//! - 切分边界落在行尾,内部不再继续切;依赖 [`crate::parser`] 已经做过基础清洗。
//! - chunks 之间行号连续不重叠,方便后续高亮回溯到原文。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::parser::Document;

/// 搜索 / 语义分析的最小文本片段 —— 见 `docs/接口设计.md` §2.3。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    /// 片段唯一 ID(`<file_path>#L<start>-L<end>`)。
    pub id: String,
    /// 来源文件路径。
    pub file_path: PathBuf,
    /// 来源文档标题(直接从 [`Document::title`] 复制)。
    pub title: Option<String>,
    /// 片段文本内容(已是 parser 清洗后的形态)。
    pub content: String,
    /// 起始行号(1-based,含)。
    pub start_line: usize,
    /// 结束行号(1-based,含)。
    pub end_line: usize,
}

/// 把单个文档切分为若干 chunk。
///
/// 行为约定:
/// - `max_chars == 0` 视为非法输入,自动夹紧到 1,避免无限循环。
/// - 长度 `<= max_chars` 的文档至少返回 1 个 chunk。
/// - 空 `content` 也返回 1 个空 chunk(`start_line = end_line = 1`),
///   下游索引侧可以选择跳过空 chunk,这里不做过滤,保证 chunk 数 >= 1。
/// - 单行长度若超过 `max_chars`,该行单独成一个 chunk(不在行内继续切,
///   保留人类可读边界)。
/// - chunks 之间行号连续不重叠:`chunks[i+1].start_line == chunks[i].end_line + 1`。
pub fn chunk_document(document: &Document, max_chars: usize) -> Vec<DocumentChunk> {
    let max_chars = max_chars.max(1);
    let lines: Vec<&str> = document.content.lines().collect();

    if lines.is_empty() {
        return vec![make_chunk(document, "", 1, 1)];
    }

    let mut chunks = Vec::new();
    let mut buf_lines: Vec<&str> = Vec::new();
    let mut buf_chars = 0usize;
    let mut start_line = 1usize;

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let line_chars = line.chars().count();

        // buf 内已有内容且本行加进来会超阈值,先 flush。
        // 加入本行将贡献 (line_chars + 1) 字符(1 是行间换行符)。
        if !buf_lines.is_empty() && buf_chars + line_chars + 1 > max_chars {
            let end_line = start_line + buf_lines.len() - 1;
            chunks.push(make_chunk(
                document,
                &buf_lines.join("\n"),
                start_line,
                end_line,
            ));
            buf_lines.clear();
            buf_chars = 0;
            start_line = line_no;
        }

        if buf_lines.is_empty() {
            buf_chars = line_chars;
        } else {
            buf_chars += line_chars + 1;
        }
        buf_lines.push(line);
    }

    // flush 最后一片。
    if !buf_lines.is_empty() {
        let end_line = start_line + buf_lines.len() - 1;
        chunks.push(make_chunk(
            document,
            &buf_lines.join("\n"),
            start_line,
            end_line,
        ));
    }

    chunks
}

fn make_chunk(doc: &Document, content: &str, start: usize, end: usize) -> DocumentChunk {
    DocumentChunk {
        id: format!("{}#L{}-L{}", doc.meta.path.display(), start, end),
        file_path: doc.meta.path.clone(),
        title: doc.title.clone(),
        content: content.to_string(),
        start_line: start,
        end_line: end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::path::Path;

    use crate::scanner::DocumentMeta;

    fn make_doc(path: &str, content: &str, title: Option<&str>) -> Document {
        let path_buf = PathBuf::from(path);
        let file_name = path_buf
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Document {
            meta: DocumentMeta {
                path: path_buf,
                file_name,
                file_size: content.len() as u64,
                modified_time: None,
            },
            title: title.map(|s| s.to_string()),
            content: content.to_string(),
        }
    }

    #[test]
    fn chunk_document_should_split_long_text() {
        // 6 行,每行 10 字符,max_chars = 20 触发切片。
        let content = "aaaaaaaaaa\nbbbbbbbbbb\ncccccccccc\ndddddddddd\neeeeeeeeee\nffffffffff";
        let doc = make_doc("/notes/x.md", content, None);
        let chunks = chunk_document(&doc, 20);

        assert!(chunks.len() >= 3, "应当切成多片,实际 {}", chunks.len());
        for chunk in &chunks {
            // 单 chunk 字符数不应超过 max_chars 的 1.5 倍(放宽以容纳一行超长场景),
            // 这里固定 max_chars=20、每行 10 字符,所以每片 <= 21(20 + 1 换行容差)。
            assert!(
                chunk.content.chars().count() <= 21,
                "chunk 过长: {} chars",
                chunk.content.chars().count()
            );
        }
    }

    #[test]
    fn chunk_short_doc_returns_one_chunk() {
        let doc = make_doc("/x.md", "short content here", None);
        let chunks = chunk_document(&doc, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "short content here");
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 1);
    }

    #[test]
    fn chunk_empty_doc_returns_one_chunk() {
        let doc = make_doc("/x.md", "", None);
        let chunks = chunk_document(&doc, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "");
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 1);
    }

    #[test]
    fn chunk_should_keep_source_path_and_title() {
        let doc = make_doc("/notes/x.md", "abc\ndef\nghi\njkl", Some("T"));
        let chunks = chunk_document(&doc, 5);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert_eq!(chunk.file_path, Path::new("/notes/x.md"));
            assert_eq!(chunk.title.as_deref(), Some("T"));
        }
    }

    #[test]
    fn chunk_ids_should_be_unique() {
        let content = "a\nb\nc\nd\ne\nf";
        let doc = make_doc("/x.md", content, None);
        let chunks = chunk_document(&doc, 2);
        let ids: HashSet<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids.len(),
            chunks.len(),
            "chunk ID 必须唯一,实际有重复: {ids:?}"
        );
    }

    #[test]
    fn chunk_id_format_should_be_path_hash_lstart_lend() {
        // max_chars=8: "abc\ndef" = 7 字符可合并; 加 "ghi" 共 11 超阈值,触发切片。
        let doc = make_doc("/notes/x.md", "abc\ndef\nghi\njkl", None);
        let chunks = chunk_document(&doc, 8);
        assert_eq!(chunks[0].id, "/notes/x.md#L1-L2");
        assert_eq!(chunks[1].id, "/notes/x.md#L3-L4");
    }

    #[test]
    fn chunk_line_ranges_should_be_contiguous_and_non_overlapping() {
        let content = "aa\nbb\ncc\ndd\nee\nff";
        let doc = make_doc("/x.md", content, None);
        let chunks = chunk_document(&doc, 4);

        assert_eq!(chunks[0].start_line, 1);
        for w in chunks.windows(2) {
            assert!(
                w[0].end_line < w[1].start_line,
                "chunks 不应重叠: {} >= {}",
                w[0].end_line,
                w[1].start_line
            );
            assert_eq!(
                w[0].end_line + 1,
                w[1].start_line,
                "chunks 之间行号应连续: {} 后跟 {}",
                w[0].end_line,
                w[1].start_line
            );
        }
        // 最后一片 end_line 应当等于总行数(content 共 6 行)。
        assert_eq!(chunks.last().unwrap().end_line, 6);
    }

    #[test]
    fn chunk_handles_zero_max_chars() {
        let doc = make_doc("/x.md", "a\nb\nc", None);
        let chunks = chunk_document(&doc, 0);
        // max_chars 被 clamp 到 1,函数应当返回且不死循环。
        assert!(!chunks.is_empty());
        let ids: HashSet<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.len(), chunks.len(), "ID 仍应唯一");
    }

    #[test]
    fn chunk_should_handle_line_longer_than_max_chars() {
        // 一行就超过 max_chars,应当单独成 chunk(不在行内切)。
        let doc = make_doc("/x.md", "aaaaaaaaaaaaaaaaaaaa\nbbb", None);
        let chunks = chunk_document(&doc, 5);
        assert!(!chunks.is_empty());
        // 长行应在自己的 chunk 里完整出现。
        assert!(chunks
            .iter()
            .any(|c| c.content.contains("aaaaaaaaaaaaaaaaaaaa")));
    }

    #[test]
    fn chunk_real_sample_should_produce_searchable_chunks() {
        // 端到端:parser -> chunker 联动,验证真实样例能切片成功且元数据传递正确。
        use crate::parser::parse_document;

        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = PathBuf::from(manifest).join("examples/sample_notes/rust_ownership.md");
        let md = std::fs::metadata(&path).expect("metadata");
        let meta = DocumentMeta {
            path: path.clone(),
            file_name: "rust_ownership.md".into(),
            file_size: md.len(),
            modified_time: md.modified().ok(),
        };

        let document = parse_document(meta).expect("parse sample");
        let chunks = chunk_document(&document, 200);

        assert!(!chunks.is_empty(), "样例应当切出至少一个 chunk");
        assert!(
            chunks
                .iter()
                .all(|c| c.title.as_deref() == Some("Rust 所有权笔记")),
            "title 必须传递到每个 chunk"
        );
        assert!(
            chunks.iter().any(|c| c.content.contains("所有权")),
            "至少一个 chunk 应包含关键词「所有权」"
        );
        assert!(
            chunks.iter().all(|c| c.file_path == path),
            "file_path 必须传递到每个 chunk"
        );
        // chunk 行号合法且不重叠。
        for w in chunks.windows(2) {
            assert!(w[0].end_line < w[1].start_line);
        }
    }

    #[test]
    fn chunk_pipeline_should_feed_inverted_index() {
        // 端到端:parser -> chunker -> indexer 跑一遍 examples/sample_notes,
        // 验证整条流水线在邱俊杰、谭张锐、陈文涛三个模块拼起来后能给出正确结果。
        use crate::indexer::InvertedIndex;
        use crate::parser::parse_document;
        use crate::scanner::scan_documents;
        use crate::search::search;

        let manifest = env!("CARGO_MANIFEST_DIR");
        let sample_dir = PathBuf::from(manifest).join("examples/sample_notes");

        let metas = scan_documents(&sample_dir).expect("scan");
        let mut all_chunks = Vec::new();
        for meta in metas {
            let doc = parse_document(meta).expect("parse");
            all_chunks.extend(chunk_document(&doc, 200));
        }
        assert!(all_chunks.len() >= 4, "至少应有 4 个 chunk");

        let index = InvertedIndex::build(&all_chunks);
        let results = search(&index, &all_chunks, "Rust", 5).expect("search ok");
        assert!(!results.is_empty(), "Rust 关键词应至少命中一条");
        assert!(
            results.iter().all(|r| r.score > 0.0),
            "命中结果分数必须 > 0"
        );

        // 中文查询也应当能命中。
        let zh = search(&index, &all_chunks, "所有权", 5).expect("search ok");
        assert!(!zh.is_empty(), "「所有权」应当至少命中一条");
    }
}
