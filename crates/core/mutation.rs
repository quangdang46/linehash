#![allow(dead_code)]

use crate::document::{Document, LineRecord};
use crate::error::LinehashError;
use crate::hash;

pub fn validate_single_line_content(content: &str) -> Result<(), LinehashError> {
    if content.contains(['\n', '\r']) {
        Err(LinehashError::MultiLineContentUnsupported)
    } else {
        Ok(())
    }
}

pub fn replace_line(
    doc: &mut Document,
    index: usize,
    content: &str,
) -> Result<(), LinehashError> {
    validate_single_line_content(content)?;
    ensure_index(doc, index)?;

    doc.lines[index].content = content.to_owned();
    rebuild_line_metadata(doc);
    Ok(())
}

pub fn replace_range_with_line(
    doc: &mut Document,
    start: usize,
    end: usize,
    content: &str,
) -> Result<(), LinehashError> {
    validate_single_line_content(content)?;
    ensure_range(doc, start, end)?;

    doc.lines.splice(start..=end, [new_line_record(content)]);
    rebuild_line_metadata(doc);
    Ok(())
}

pub fn insert_line(
    doc: &mut Document,
    index: usize,
    content: &str,
) -> Result<(), LinehashError> {
    validate_single_line_content(content)?;
    ensure_insert_index(doc, index)?;

    doc.lines.insert(index, new_line_record(content));
    rebuild_line_metadata(doc);
    Ok(())
}

pub fn delete_line(doc: &mut Document, index: usize) -> Result<(), LinehashError> {
    ensure_index(doc, index)?;

    doc.lines.remove(index);
    rebuild_line_metadata(doc);
    Ok(())
}

fn rebuild_line_metadata(doc: &mut Document) {
    for (index, line) in doc.lines.iter_mut().enumerate() {
        line.number = index + 1;
        line.full_hash = hash::full_hash(&line.content);
        line.short_hash = hash::short_from_full(line.full_hash);
    }
}

fn new_line_record(content: &str) -> LineRecord {
    let full_hash = hash::full_hash(content);
    LineRecord {
        number: 0,
        content: content.to_owned(),
        full_hash,
        short_hash: hash::short_from_full(full_hash),
    }
}

fn ensure_index(doc: &Document, index: usize) -> Result<(), LinehashError> {
    if index < doc.lines.len() {
        Ok(())
    } else {
        Err(LinehashError::MutationIndexOutOfBounds {
            index,
            len: doc.lines.len(),
        })
    }
}

fn ensure_insert_index(doc: &Document, index: usize) -> Result<(), LinehashError> {
    if index <= doc.lines.len() {
        Ok(())
    } else {
        Err(LinehashError::MutationIndexOutOfBounds {
            index,
            len: doc.lines.len(),
        })
    }
}

fn ensure_range(doc: &Document, start: usize, end: usize) -> Result<(), LinehashError> {
    if start <= end && end < doc.lines.len() {
        Ok(())
    } else {
        Err(LinehashError::InvalidMutationRange {
            start,
            end,
            len: doc.lines.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        delete_line, insert_line, replace_line, replace_range_with_line,
        validate_single_line_content,
    };
    use crate::document::{Document, NewlineStyle};
    use crate::error::LinehashError;
    use std::path::Path;

    #[test]
    fn replace_line_recomputes_hashes_and_preserves_document_flags() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();
        let original_newline = doc.newline;
        let original_trailing_newline = doc.trailing_newline;

        replace_line(&mut doc, 1, "gamma").unwrap();

        assert_eq!(doc.lines[1].content, "gamma");
        assert_eq!(doc.lines[1].number, 2);
        assert_eq!(doc.newline, original_newline);
        assert_eq!(doc.trailing_newline, original_trailing_newline);
        assert_eq!(doc.render(), b"alpha\ngamma\n");
    }

    #[test]
    fn replace_range_collapses_to_single_line() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\ngamma\ndelta\n")
            .unwrap();

        replace_range_with_line(&mut doc, 1, 2, "merged").unwrap();

        assert_eq!(doc.lines.len(), 3);
        assert_eq!(doc.lines[0].content, "alpha");
        assert_eq!(doc.lines[1].content, "merged");
        assert_eq!(doc.lines[2].content, "delta");
        assert_eq!(doc.render(), b"alpha\nmerged\ndelta\n");
    }

    #[test]
    fn insert_line_at_index_renumbers_following_lines() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\ngamma\n").unwrap();

        insert_line(&mut doc, 1, "beta").unwrap();

        assert_eq!(doc.lines.len(), 3);
        assert_eq!(doc.lines[1].content, "beta");
        assert_eq!(doc.lines[1].number, 2);
        assert_eq!(doc.lines[2].content, "gamma");
        assert_eq!(doc.lines[2].number, 3);
        assert_eq!(doc.render(), b"alpha\nbeta\ngamma\n");
    }

    #[test]
    fn insert_line_allows_appending_to_end() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\n").unwrap();

        insert_line(&mut doc, 1, "beta").unwrap();

        assert_eq!(doc.render(), b"alpha\nbeta\n");
    }

    #[test]
    fn delete_line_removes_middle_line() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\ngamma\n").unwrap();

        delete_line(&mut doc, 1).unwrap();

        assert_eq!(doc.lines.len(), 2);
        assert_eq!(doc.lines[0].content, "alpha");
        assert_eq!(doc.lines[1].content, "gamma");
        assert_eq!(doc.lines[1].number, 2);
        assert_eq!(doc.render(), b"alpha\ngamma\n");
    }

    #[test]
    fn delete_last_remaining_line_produces_empty_document() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha").unwrap();

        delete_line(&mut doc, 0).unwrap();

        assert!(doc.lines.is_empty());
        assert_eq!(doc.render(), b"");
        assert!(!doc.trailing_newline);
    }

    #[test]
    fn preserves_crlf_and_trailing_newline_flags_through_mutation() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\r\nbeta\r\n").unwrap();

        insert_line(&mut doc, 1, "middle").unwrap();

        assert_eq!(doc.newline, NewlineStyle::Crlf);
        assert!(doc.trailing_newline);
        assert_eq!(doc.render(), b"alpha\r\nmiddle\r\nbeta\r\n");
    }

    #[test]
    fn multiline_content_is_rejected() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\n").unwrap();

        let error = replace_line(&mut doc, 0, "beta\ngamma").unwrap_err();
        assert!(matches!(error, LinehashError::MultiLineContentUnsupported));
    }

    #[test]
    fn invalid_indices_are_rejected() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\n").unwrap();

        let error = delete_line(&mut doc, 1).unwrap_err();
        assert!(matches!(
            error,
            LinehashError::MutationIndexOutOfBounds { index: 1, len: 1 }
        ));
    }

    #[test]
    fn invalid_range_is_rejected() {
        let mut doc = Document::from_str(Path::new("demo.txt"), "alpha\nbeta\n").unwrap();

        let error = replace_range_with_line(&mut doc, 1, 2, "gamma").unwrap_err();
        assert!(matches!(
            error,
            LinehashError::InvalidMutationRange {
                start: 1,
                end: 2,
                len: 2
            }
        ));
    }

    #[test]
    fn validate_single_line_content_rejects_carriage_return() {
        let error = validate_single_line_content("alpha\rbeta").unwrap_err();
        assert!(matches!(error, LinehashError::MultiLineContentUnsupported));
    }
}
