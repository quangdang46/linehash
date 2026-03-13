#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::error::LinehashError;
use crate::hash::{self, ShortHash};

pub type ShortHashIndex = Vec<Vec<usize>>;

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
    pub content: String,
    pub full_hash: u32,
    pub short_hash: ShortHash,
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

    pub fn build_index(&self) -> ShortHashIndex {
        let counts = count_short_hashes(&self.lines);
        build_index_from_counts(&self.lines, &counts)
    }

    pub fn render(&self) -> Vec<u8> {
        if self.lines.is_empty() {
            return Vec::new();
        }

        let separator = self.newline.separator().as_bytes();
        let content_len: usize = self.lines.iter().map(|line| line.content.len()).sum();
        let separator_count =
            self.lines.len().saturating_sub(1) + usize::from(self.trailing_newline);
        let mut rendered = Vec::with_capacity(content_len + separator.len() * separator_count);

        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                rendered.extend_from_slice(separator);
            }
            rendered.extend_from_slice(line.content.as_bytes());
        }

        if self.trailing_newline {
            rendered.extend_from_slice(separator);
        }

        rendered
    }

    pub fn compute_stats(&self) -> FileStats {
        let bucket_counts = count_short_hashes(&self.lines);
        let index = build_index_from_counts(&self.lines, &bucket_counts);
        let mut collision_pairs = collect_collision_pairs(&index);
        collision_pairs.sort_unstable();
        let (unique_hashes, collision_count) = summarize_bucket_counts(&bucket_counts);

        FileStats {
            line_count: self.len(),
            unique_hashes,
            collision_count,
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

pub fn format_short_hash(short_hash: ShortHash) -> String {
    hash::format_short_hash(short_hash)
}

fn build_lines(content: &str, newline: NewlineStyle) -> Vec<LineRecord> {
    if content.is_empty() {
        return Vec::new();
    }

    let line_count = match newline {
        NewlineStyle::Lf => content
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count(),
        NewlineStyle::Crlf => content
            .as_bytes()
            .windows(2)
            .filter(|window| *window == b"\r\n")
            .count(),
    };
    let mut lines = Vec::with_capacity(line_count);

    match newline {
        NewlineStyle::Lf => {
            for line in content.split_terminator('\n') {
                lines.push(build_line_record(line));
            }
        }
        NewlineStyle::Crlf => {
            for line in content.split_terminator("\r\n") {
                lines.push(build_line_record(line));
            }
        }
    }

    lines
}

fn build_line_record(content: &str) -> LineRecord {
    let full_hash = hash::full_hash(content);
    LineRecord {
        content: content.to_owned(),
        full_hash,
        short_hash: hash::short_from_full(full_hash),
    }
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

fn empty_index() -> ShortHashIndex {
    vec![Vec::new(); 256]
}

fn count_short_hashes(lines: &[LineRecord]) -> [usize; 256] {
    let mut counts = [0; 256];
    for line in lines {
        counts[line.short_hash as usize] += 1;
    }
    counts
}

fn build_index_from_counts(lines: &[LineRecord], counts: &[usize; 256]) -> ShortHashIndex {
    let mut index = empty_index();
    for (bucket, count) in counts.iter().enumerate() {
        if *count > 0 {
            index[bucket] = Vec::with_capacity(*count);
        }
    }

    for (line_index, line) in lines.iter().enumerate() {
        index[line.short_hash as usize].push(line_index);
    }

    index
}

fn summarize_bucket_counts(counts: &[usize; 256]) -> (usize, usize) {
    let mut unique_hashes = 0;
    let mut collision_count = 0;

    for count in counts {
        if *count == 0 {
            continue;
        }
        unique_hashes += 1;
        if *count >= 2 {
            collision_count += *count;
        }
    }

    (unique_hashes, collision_count)
}

fn collect_collision_pairs(index: &ShortHashIndex) -> Vec<(usize, usize)> {
    let capacity = index
        .iter()
        .filter(|positions| positions.len() >= 2)
        .map(|positions| positions.len() * (positions.len() - 1) / 2)
        .sum();
    let mut pairs = Vec::with_capacity(capacity);

    for positions in index.iter().filter(|positions| positions.len() >= 2) {
        for left in 0..positions.len() {
            for right in left + 1..positions.len() {
                pairs.push((positions[left] + 1, positions[right] + 1));
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
    use super::{Document, FileStats, NewlineStyle, format_short_hash};
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
    fn test_load_single_line_no_trailing_newline() {
        let (_dir, path) = write_temp_file("alpha");
        let document = Document::load(&path).unwrap();

        assert_eq!(document.lines.len(), 1);
        assert_eq!(document.lines[0].content, "alpha");
        assert!(!document.trailing_newline);
    }

    #[test]
    fn test_load_single_line_with_trailing_newline() {
        let (_dir, path) = write_temp_file("alpha\n");
        let document = Document::load(&path).unwrap();

        assert_eq!(document.lines.len(), 1);
        assert_eq!(document.lines[0].content, "alpha");
        assert!(document.trailing_newline);
    }

    #[test]
    fn test_load_empty_file() {
        let (_dir, path) = write_temp_file("");
        let document = Document::load(&path).unwrap();

        assert!(document.lines.is_empty());
        assert!(!document.trailing_newline);
    }

    #[test]
    fn test_load_whitespace_only_lines() {
        let (_dir, path) = write_temp_file("  \n\t\n");
        let document = Document::load(&path).unwrap();

        assert_eq!(document.lines.len(), 2);
        assert_eq!(document.lines[0].content, "  ");
        assert_eq!(document.lines[1].content, "\t");
    }

    #[test]
    fn test_load_mixed_newlines_fails() {
        let (_dir, path) = write_temp_file("alpha\r\nbeta\n");
        let error = Document::load(&path).unwrap_err();

        assert!(matches!(error, LinehashError::MixedNewlines { .. }));
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
    fn test_load_binary_file_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("binary.bin");
        fs::write(&path, b"abc\0def").unwrap();

        let error = Document::load(&path).unwrap_err();
        assert!(matches!(error, LinehashError::BinaryFile { .. }));
    }

    #[test]
    fn test_binary_check_precedes_utf8_error_when_nul_is_present() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("binary-or-invalid.bin");
        let bytes = vec![0xff, 0x00, 0xfe];
        fs::write(&path, bytes).unwrap();

        let error = Document::load(&path).unwrap_err();
        assert!(matches!(error, LinehashError::BinaryFile { .. }));
    }

    #[test]
    fn test_binary_file_hint_matches_product_wording() {
        let error = LinehashError::BinaryFile {
            path: "demo.bin".to_owned(),
        };
        assert_eq!(
            error.hint(),
            Some("linehash only supports UTF-8 text files")
        );
    }

    #[test]
    fn test_render_lf_round_trip() {
        let doc = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        assert_eq!(doc.render(), b"alpha\nbeta\n");
    }

    #[test]
    fn test_render_crlf_round_trip() {
        let doc = Document::from_str(Path::new("demo.txt"), "alpha\r\nbeta\r\n").unwrap();
        assert_eq!(doc.render(), b"alpha\r\nbeta\r\n");
    }

    #[test]
    fn test_render_no_trailing_newline_preserved() {
        let doc = Document::from_str(Path::new("demo.txt"), "alpha\nbeta").unwrap();
        assert_eq!(doc.render(), b"alpha\nbeta");
    }

    #[test]
    fn test_render_trailing_newline_preserved() {
        let doc = Document::from_str(Path::new("demo.txt"), "alpha\n").unwrap();
        assert_eq!(doc.render(), b"alpha\n");
    }

    #[test]
    fn test_render_empty_document_is_empty_bytes() {
        let doc = Document::from_str(Path::new("demo.txt"), "").unwrap();
        assert!(doc.render().is_empty());
    }

    #[test]
    fn test_line_order_matches_vector_positions() {
        let doc = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        assert_eq!(doc.lines[0].content, "alpha");
        assert_eq!(doc.lines[1].content, "beta");
    }

    #[test]
    fn test_build_index_unique_hashes() {
        let doc = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\ngamma\n").unwrap();
        let index = doc.build_index();
        let alpha_hash = doc.lines[0].short_hash as usize;
        let beta_hash = doc.lines[1].short_hash as usize;
        assert_eq!(index[alpha_hash], vec![0]);
        assert_eq!(index[beta_hash], vec![1]);
    }

    #[test]
    fn test_build_index_collision_has_multiple_entries() {
        let (first, second) = find_collision_pair();
        let doc =
            Document::from_str(Path::new("demo.txt"), &format!("{first}\n{second}\n")).unwrap();
        let index = doc.build_index();
        let short = doc.lines[0].short_hash as usize;
        assert_eq!(index[short], vec![0, 1]);
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
        let document = Document::from_str(
            Path::new("demo.txt"),
            &format!("{first}\n{second}\nunique\n"),
        )
        .unwrap();
        let stats = document.compute_stats();
        assert_eq!(stats.collision_count, 2);
        assert_eq!(stats.collision_pairs, vec![(1, 2)]);
    }

    #[test]
    fn test_token_estimate_proportional_to_size() {
        let short = Document::from_str(Path::new("demo.txt"), "a\n").unwrap();
        let long = Document::from_str(Path::new("demo.txt"), "a very long line indeed\n").unwrap();
        assert!(
            long.compute_stats().estimated_read_tokens
                > short.compute_stats().estimated_read_tokens
        );
    }

    #[test]
    fn test_hash_length_advice_2_for_small_file() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        assert_eq!(document.compute_stats().hash_length_advice, 2);
    }

    #[test]
    fn test_hash_length_advice_4_for_medium_file() {
        let content = (0..200)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let document = Document::from_str(Path::new("demo.txt"), &content).unwrap();
        assert_eq!(document.compute_stats().hash_length_advice, 4);
    }

    #[test]
    fn test_context_suggestion_minimum_3_with_dense_markers() {
        let document =
            Document::from_str(Path::new("demo.txt"), "fn a\nfn b\nfn c\nfn d\n").unwrap();
        assert_eq!(document.compute_stats().suggested_context_n, 3);
    }

    #[test]
    fn test_context_suggestion_falls_back_to_5_without_markers() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\ngamma\n").unwrap();
        assert_eq!(document.compute_stats().suggested_context_n, 5);
    }

    #[test]
    fn test_context_suggestion_capped_at_20() {
        let mut lines = (0..100).map(|i| format!("line-{i}")).collect::<Vec<_>>();
        lines.insert(0, String::from("fn a"));
        lines.push(String::from("fn b"));
        let document =
            Document::from_str(Path::new("demo.txt"), &(lines.join("\n") + "\n")).unwrap();
        assert_eq!(document.compute_stats().suggested_context_n, 20);
    }

    #[test]
    fn test_filemeta_captured() {
        let (_dir, path) = write_temp_file("alpha\n");
        let document = Document::load(&path).unwrap();

        let meta = document.file_meta.expect("metadata should be present");
        assert!(meta.mtime_secs > 0);
        #[cfg(unix)]
        assert!(meta.inode > 0);
    }

    #[test]
    fn test_short_hash_formatting_round_trip() {
        let document = Document::from_str(Path::new("demo.txt"), "alpha\n").unwrap();
        assert_eq!(format_short_hash(document.lines[0].short_hash).len(), 2);
    }

    fn write_temp_file(content: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("demo.txt");
        fs::write(&path, content).unwrap();
        (dir, path)
    }

    fn find_collision_pair() -> (String, String) {
        for i in 0..10_000 {
            let left = format!("line-{i}");
            for j in (i + 1)..10_000 {
                let right = format!("line-{j}");
                let doc = Document::from_str(Path::new("demo.txt"), &format!("{left}\n{right}\n"))
                    .unwrap();
                if doc.lines[0].short_hash == doc.lines[1].short_hash {
                    return (left, right);
                }
            }
        }
        panic!("failed to find a collision doc");
    }
}
