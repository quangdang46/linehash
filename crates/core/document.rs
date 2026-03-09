#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

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
        let content = String::from_utf8(bytes.clone()).map_err(|_| LinehashError::InvalidUtf8 {
            path: path_string.clone(),
        })?;

        if bytes.iter().take(8_000).any(|byte| *byte == 0) {
            return Err(LinehashError::BinaryFile { path: path_string });
        }

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
    use super::{Document, NewlineStyle};
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
    fn test_load_binary_file_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("binary.txt");
        fs::write(&path, b"abc\0def").unwrap();

        let error = Document::load(&path).unwrap_err();
        assert!(matches!(error, LinehashError::BinaryFile { .. }));
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
