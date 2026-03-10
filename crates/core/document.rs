#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::error::LinehashError;
use crate::hash;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewlineStyle {
    Lf,
    Crlf,
}

impl NewlineStyle {
    fn separator(self) -> &'static str {
        match self {
            NewlineStyle::Lf => "\n",
            NewlineStyle::Crlf => "\r\n",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileMeta {
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
    pub inode: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineRecord {
    pub number: usize,
    pub content: String,
    pub full_hash: u32,
    pub short_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileStats {
    pub line_count: usize,
    pub unique_hashes: usize,
    pub collision_count: usize,
    pub collision_pairs: Vec<(usize, usize)>,
    pub estimated_read_tokens: usize,
    pub hash_length_advice: u8,
    pub suggested_context_n: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    pub path: PathBuf,
    pub newline: NewlineStyle,
    pub trailing_newline: bool,
    pub lines: Vec<LineRecord>,
    pub file_meta: Option<FileMeta>,
}

impl Document {
    pub fn load(path: &Path) -> Result<Document, LinehashError> {
        let bytes = fs::read(path)?;
        let path_string = path.display().to_string();

        if bytes.iter().take(8_000).any(|byte| *byte == 0) {
            return Err(LinehashError::BinaryFile { path: path_string });
        }

        let content = String::from_utf8(bytes).map_err(|_| LinehashError::InvalidUtf8 {
            path: path_string.clone(),
        })?;

        let newline = detect_newline_style(&content, path)?;
        let trailing_newline = content.ends_with('\n');
        let lines = build_lines(&content, newline);
        let metadata = fs::metadata(path)?;
        let file_meta = Some(FileMeta::from_metadata(&metadata)?);

        Ok(Document {
            path: path.to_path_buf(),
            newline,
            trailing_newline,
            lines,
            file_meta,
        })
    }

    pub fn from_str(path: &Path, content: &str) -> Result<Document, LinehashError> {
        let newline = detect_newline_style(content, path)?;
        let trailing_newline = content.ends_with('\n');
        let lines = build_lines(content, newline);

        Ok(Document {
            path: path.to_path_buf(),
            newline,
            trailing_newline,
            lines,
            file_meta: None,
        })
    }

    pub fn build_index(&self) -> HashMap<String, Vec<usize>> {
        let mut index = HashMap::new();
        for (line_index, line) in self.lines.iter().enumerate() {
            index
                .entry(line.short_hash.clone())
                .or_insert_with(Vec::new)
                .push(line_index);
        }
        index
    }

    pub fn render(&self) -> Vec<u8> {
        if self.lines.is_empty() {
            return Vec::new();
        }

        let mut rendered = self
            .lines
            .iter()
            .map(|line| line.content.as_str())
            .collect::<Vec<_>>()
            .join(self.newline.separator());

        if self.trailing_newline {
            rendered.push_str(self.newline.separator());
        }

        rendered.into_bytes()
    }

    pub fn compute_stats(&self) -> FileStats {
        let index = self.build_index();
        let mut collision_pairs = collect_collision_pairs(self, &index);
        collision_pairs.sort_unstable();

        FileStats {
            line_count: self.len(),
            unique_hashes: index.len(),
            collision_count: index
                .values()
                .filter(|positions| positions.len() >= 2)
                .map(Vec::len)
                .sum(),
            collision_pairs,
            estimated_read_tokens: estimate_read_tokens(self),
            hash_length_advice: recommend_hash_length(self),
            suggested_context_n: suggest_context_n(self),
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

impl FileMeta {
    fn from_metadata(metadata: &fs::Metadata) -> Result<Self, LinehashError> {
        let modified = metadata.modified()?;
        let duration = modified.duration_since(UNIX_EPOCH).unwrap_or_default();

        Ok(Self {
            mtime_secs: duration.as_secs() as i64,
            mtime_nanos: duration.subsec_nanos(),
            inode: inode_from_metadata(metadata),
        })
    }
}

fn build_lines(content: &str, newline: NewlineStyle) -> Vec<LineRecord> {
    let raw_lines: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else {
        match newline {
            NewlineStyle::Lf => content.split_terminator('\n').collect(),
            NewlineStyle::Crlf => content.split_terminator("\r\n").collect(),
        }
    };

    raw_lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let full_hash = hash::full_hash(line);
            LineRecord {
                number: index + 1,
                content: line.to_owned(),
                full_hash,
                short_hash: hash::short_from_full(full_hash),
            }
        })
        .collect()
}

fn detect_newline_style(content: &str, path: &Path) -> Result<NewlineStyle, LinehashError> {
    let bytes = content.as_bytes();
    let mut saw_lf = false;
    let mut saw_crlf = false;
    let mut saw_bare_cr = false;
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    saw_crlf = true;
                    index += 2;
                } else {
                    saw_bare_cr = true;
                    index += 1;
                }
            }
            b'\n' => {
                saw_lf = true;
                index += 1;
            }
            _ => {
                index += 1;
            }
        }
    }

    if saw_bare_cr || (saw_crlf && saw_lf) {
        return Err(LinehashError::MixedNewlines {
            path: path.display().to_string(),
        });
    }

    if saw_crlf {
        Ok(NewlineStyle::Crlf)
    } else {
        Ok(NewlineStyle::Lf)
    }
}

fn collect_collision_pairs(doc: &Document, index: &HashMap<String, Vec<usize>>) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();

    for positions in index.values().filter(|positions| positions.len() >= 2) {
        for left in 0..positions.len() {
            for right in left + 1..positions.len() {
                pairs.push((
                    doc.lines[positions[left]].number,
                    doc.lines[positions[right]].number,
                ));
            }
        }
    }

    pairs
}

fn estimate_read_tokens(doc: &Document) -> usize {
    let content_chars: usize = doc.lines.iter().map(|line| line.content.len()).sum();
    let anchor_overhead = doc.lines.len() * 8;
    (content_chars + anchor_overhead) / 4
}

fn recommend_hash_length(doc: &Document) -> u8 {
    let line_count = doc.len();
    for hash_len in [2_u8, 3, 4] {
        let buckets = 16_f64.powi(i32::from(hash_len));
        if collision_probability(line_count, buckets) < 0.01 {
            return hash_len;
        }
    }
    4
}

fn collision_probability(line_count: usize, buckets: f64) -> f64 {
    if line_count <= 1 {
        return 0.0;
    }

    let line_count = line_count as f64;
    1.0 - (-(line_count * (line_count - 1.0)) / (2.0 * buckets)).exp()
}

fn suggest_context_n(doc: &Document) -> usize {
    let markers = doc
        .lines
        .iter()
        .map(|line| line.content.as_str())
        .enumerate()
        .filter_map(|(index, content)| is_structure_marker(content).then_some(index + 1))
        .collect::<Vec<_>>();

    if markers.len() < 2 {
        return 5;
    }

    let mut gaps = markers
        .windows(2)
        .map(|window| window[1] - window[0])
        .collect::<Vec<_>>();
    gaps.sort_unstable();
    let median_gap = gaps[gaps.len() / 2];
    (median_gap / 2).clamp(3, 20)
}

fn is_structure_marker(content: &str) -> bool {
    ["function ", "def ", "class ", "fn ", "impl "]
        .iter()
        .any(|marker| content.contains(marker))
}

#[cfg(unix)]
fn inode_from_metadata(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;

    metadata.ino()
}

#[cfg(not(unix))]
fn inode_from_metadata(_metadata: &fs::Metadata) -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::{Document, FileStats, NewlineStyle};
    use crate::error::LinehashError;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    #[test]
    fn test_load_lf_simple() {
        let (_dir, path) = write_temp_file("alpha\nbeta\n");
        let document = Document::load(&path).unwrap();

        assert_eq!(document.newline, NewlineStyle::Lf);
        assert!(document.trailing_newline);
        assert_eq!(document.lines.len(), 2);
        assert_eq!(document.lines[0].content, "alpha");
        assert_eq!(document.lines[1].content, "beta");
    }

    #[test]
    fn test_load_crlf_simple() {
        let (_dir, path) = write_temp_file("alpha\r\nbeta\r\n");
        let document = Document::load(&path).unwrap();

        assert_eq!(document.newline, NewlineStyle::Crlf);
        assert!(document.trailing_newline);
        assert_eq!(document.lines.len(), 2);
        assert_eq!(document.lines[1].content, "beta");
    }

    #[test]
    fn test_load_mixed_newlines_fails() {
        let (_dir, path) = write_temp_file("alpha\nbeta\r\n");
        let error = Document::load(&path).unwrap_err();

        assert!(matches!(error, LinehashError::MixedNewlines { .. }));
    }

    #[test]
    fn test_load_empty_file() {
        let (_dir, path) = write_temp_file("");
        let document = Document::load(&path).unwrap();

        assert!(document.is_empty());
        assert!(!document.trailing_newline);
        assert_eq!(document.render(), b"");
    }

    #[test]
    fn test_load_single_line_no_trailing_newline() {
        let (_dir, path) = write_temp_file("alpha");
        let document = Document::load(&path).unwrap();

        assert_eq!(document.len(), 1);
        assert!(!document.trailing_newline);
    }

    #[test]
    fn test_load_single_line_with_trailing_newline() {
        let (_dir, path) = write_temp_file("alpha\n");
        let document = Document::load(&path).unwrap();

        assert_eq!(document.len(), 1);
        assert!(document.trailing_newline);
    }

    #[test]
    fn test_load_whitespace_only_lines() {
        let (_dir, path) = write_temp_file("  \n\t\n");
        let document = Document::load(&path).unwrap();

        assert_eq!(document.lines[0].content, "  ");
        assert_eq!(document.lines[1].content, "\t");
        assert_ne!(document.lines[0].short_hash, document.lines[1].short_hash);
    }

    #[test]
    fn test_load_invalid_utf8_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("invalid.txt");
        fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();

        let error = Document::load(&path).unwrap_err();
        assert!(matches!(error, LinehashError::InvalidUtf8 { .. }));
    }

    #[test]
    fn test_binary_check_precedes_utf8_error_when_nul_is_present() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("binary-invalid.txt");
        fs::write(&path, [0xff, 0x00, 0xfe]).unwrap();

        let error = Document::load(&path).unwrap_err();
        assert!(matches!(error, LinehashError::BinaryFile { .. }));
    }

    #[test]
    fn test_load_binary_file_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("binary.txt");
        fs::write(&path, b"abc\0def").unwrap();

        let error = Document::load(&path).unwrap_err();
        assert!(matches!(error, LinehashError::BinaryFile { .. }));
    }

    #[test]
    fn test_binary_file_hint_matches_product_wording() {
        let error = LinehashError::BinaryFile {
            path: "demo.bin".into(),
        };

        assert_eq!(error.hint(), Some("linehash only supports UTF-8 text files"));
    }

    #[test]
    fn test_render_lf_round_trip() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        assert_eq!(document.render(), b"alpha\nbeta\n");
    }

    #[test]
    fn test_render_crlf_round_trip() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\r\nbeta\r\n").unwrap();
        assert_eq!(document.render(), b"alpha\r\nbeta\r\n");
    }

    #[test]
    fn test_render_no_trailing_newline_preserved() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta").unwrap();
        assert_eq!(document.render(), b"alpha\nbeta");
    }

    #[test]
    fn test_render_trailing_newline_preserved() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        assert_eq!(document.render(), b"alpha\nbeta\n");
    }

    #[test]
    fn test_render_empty_document_is_empty_bytes() {
        let document = Document::from_str(Path::new("demo.txt"), "").unwrap();
        assert_eq!(document.render(), b"");
    }

    #[test]
    fn test_build_index_unique_hashes() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        let index = document.build_index();

        assert_eq!(index.len(), 2);
        assert!(index.values().all(|positions| positions.len() == 1));
    }

    #[test]
    fn test_build_index_collision_has_multiple_entries() {
        let (first, second) = find_collision_pair();
        let document =
            Document::from_str(Path::new("demo.txt"), &format!("{first}\n{second}\n")).unwrap();
        let index = document.build_index();
        let hash = document.lines[0].short_hash.clone();

        assert_eq!(index.get(&hash), Some(&vec![0, 1]));
    }

    #[test]
    fn test_line_numbers_are_1_based() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        assert_eq!(document.lines[0].number, 1);
        assert_eq!(document.lines[1].number, 2);
    }

    #[test]
    fn test_filemeta_captured() {
        let (_dir, path) = write_temp_file("alpha\n");
        let document = Document::load(&path).unwrap();

        let metadata = document
            .file_meta
            .expect("file metadata should be captured");
        assert!(metadata.mtime_secs >= 0);
    }

    #[test]
    fn test_empty_file_stats() {
        let document = Document::from_str(Path::new("demo.txt"), "").unwrap();
        let stats = document.compute_stats();
        assert_eq!(
            stats,
            FileStats {
                line_count: 0,
                unique_hashes: 0,
                collision_count: 0,
                collision_pairs: vec![],
                estimated_read_tokens: 0,
                hash_length_advice: 2,
                suggested_context_n: 5,
            }
        );
    }

    #[test]
    fn test_no_collisions_file_stats() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        let stats = document.compute_stats();
        assert_eq!(stats.line_count, 2);
        assert_eq!(stats.unique_hashes, 2);
        assert_eq!(stats.collision_count, 0);
        assert!(stats.collision_pairs.is_empty());
    }

    #[test]
    fn test_collision_count_and_pairs_correct() {
        let (first, second) = find_collision_pair();
        let document =
            Document::from_str(Path::new("demo.txt"), &format!("{first}\n{second}\nunique\n")).unwrap();
        let stats = document.compute_stats();
        assert_eq!(stats.collision_count, 2);
        assert_eq!(stats.collision_pairs, vec![(1, 2)]);
    }

    #[test]
    fn test_token_estimate_proportional_to_size() {
        let short = Document::from_str(Path::new("demo.txt"), "a\n").unwrap();
        let long = Document::from_str(Path::new("demo.txt"), "a very long line indeed\n").unwrap();
        assert!(
            long.compute_stats().estimated_read_tokens > short.compute_stats().estimated_read_tokens
        );
    }

    #[test]
    fn test_hash_length_advice_2_for_small_file() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        assert_eq!(document.compute_stats().hash_length_advice, 2);
    }

    #[test]
    fn test_hash_length_advice_4_for_medium_file() {
        let content = (1..=40)
            .map(|n| format!("line-{n}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let document = Document::from_str(Path::new("demo.txt"), &content).unwrap();
        assert_eq!(document.compute_stats().hash_length_advice, 4);
    }

    #[test]
    fn test_context_suggestion_minimum_3_with_dense_markers() {
        let document = Document::from_str(
            Path::new("demo.txt"),
            "fn a\nfn b\nfn c\nfn d\n",
        )
        .unwrap();
        assert_eq!(document.compute_stats().suggested_context_n, 3);
    }

    #[test]
    fn test_context_suggestion_falls_back_to_5_without_markers() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\ngamma\n").unwrap();
        assert_eq!(document.compute_stats().suggested_context_n, 5);
    }

    #[test]
    fn test_context_suggestion_capped_at_20() {
        let mut lines = vec![String::from("fn a")];
        lines.extend((0..50).map(|n| format!("line-{n}")));
        lines.push(String::from("fn b"));
        let document = Document::from_str(Path::new("demo.txt"), &(lines.join("\n") + "\n")).unwrap();
        assert_eq!(document.compute_stats().suggested_context_n, 20);
    }

    fn write_temp_file(content: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("fixture.txt");
        fs::write(&path, content).unwrap();
        (dir, path)
    }

    fn find_collision_pair() -> (String, String) {
        for i in 0..10_000 {
            let left = format!("line-{i}");
            for j in (i + 1)..10_000 {
                let right = format!("line-{j}");
                let left_doc = Document::from_str(Path::new("demo.txt"), &left).unwrap();
                let right_doc = Document::from_str(Path::new("demo.txt"), &right).unwrap();
                if left_doc.lines[0].short_hash == right_doc.lines[0].short_hash {
                    return (left, right);
                }
            }
        }
        panic!("failed to find a collision pair");
    }
}
