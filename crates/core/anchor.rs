#![allow(dead_code)]

use std::collections::HashMap;

use crate::document::Document;
use crate::error::LinehashError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Anchor {
    Hash { short: String },
    LineHash { line: usize, short: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeAnchor {
    pub start: Anchor,
    pub end: Anchor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLine {
    pub index: usize,
    pub line_no: usize,
    pub short_hash: String,
}

pub fn parse_anchor(s: &str) -> Result<Anchor, LinehashError> {
    let normalized = normalize_anchor_input(s);

    if normalized.contains("..") {
        return Err(LinehashError::InvalidAnchor {
            anchor: s.trim().to_owned(),
        });
    }

    if let Some((line, short)) = normalized.split_once(':') {
        let line = parse_line_number(line, s)?;
        validate_short_hash(short, s)?;
        return Ok(Anchor::LineHash {
            line,
            short: short.to_owned(),
        });
    }

    validate_short_hash(&normalized, s)?;
    Ok(Anchor::Hash { short: normalized })
}

pub fn parse_range(s: &str) -> Result<RangeAnchor, LinehashError> {
    let normalized = normalize_anchor_input(s);
    let (left, right) = normalized
        .split_once("..")
        .ok_or_else(|| LinehashError::InvalidRange {
            range: s.trim().to_owned(),
        })?;

    if right.contains("..") {
        return Err(LinehashError::InvalidRange {
            range: s.trim().to_owned(),
        });
    }

    let start = parse_anchor(left).map_err(|_| LinehashError::InvalidRange {
        range: s.trim().to_owned(),
    })?;
    let end = parse_anchor(right).map_err(|_| LinehashError::InvalidRange {
        range: s.trim().to_owned(),
    })?;

    if !matches!(start, Anchor::LineHash { .. }) || !matches!(end, Anchor::LineHash { .. }) {
        return Err(LinehashError::InvalidRange {
            range: s.trim().to_owned(),
        });
    }

    Ok(RangeAnchor { start, end })
}

pub fn resolve(
    anchor: &Anchor,
    doc: &Document,
    index: &HashMap<String, Vec<usize>>,
) -> Result<ResolvedLine, LinehashError> {
    match anchor {
        Anchor::Hash { short } => resolve_unqualified(short, doc, index),
        Anchor::LineHash { line, short } => resolve_qualified(*line, short, doc, index),
    }
}

pub fn resolve_range(
    range: &RangeAnchor,
    doc: &Document,
    index: &HashMap<String, Vec<usize>>,
) -> Result<(ResolvedLine, ResolvedLine), LinehashError> {
    let start = resolve(&range.start, doc, index)?;
    let end = resolve(&range.end, doc, index)?;

    if start.index > end.index {
        return Err(LinehashError::InvalidRange {
            range: format!(
                "{}..{}",
                display_anchor(&range.start),
                display_anchor(&range.end)
            ),
        });
    }

    Ok((start, end))
}

pub fn resolve_all(
    anchors: &[Anchor],
    doc: &Document,
    index: &HashMap<String, Vec<usize>>,
) -> Vec<Result<ResolvedLine, LinehashError>> {
    anchors
        .iter()
        .map(|anchor| resolve(anchor, doc, index))
        .collect()
}

fn resolve_unqualified(
    short: &str,
    doc: &Document,
    index: &HashMap<String, Vec<usize>>,
) -> Result<ResolvedLine, LinehashError> {
    let path = doc.path.display().to_string();
    match index.get(short) {
        None => Err(LinehashError::HashNotFound {
            hash: short.to_owned(),
            path,
        }),
        Some(matches) if matches.len() == 1 => {
            let resolved_index = matches[0];
            Ok(ResolvedLine {
                index: resolved_index,
                line_no: resolved_index + 1,
                short_hash: short.to_owned(),
            })
        }
        Some(matches) => Err(LinehashError::AmbiguousHash {
            hash: short.to_owned(),
            count: matches.len(),
            lines: matches
                .iter()
                .map(|idx| (idx + 1).to_string())
                .collect::<Vec<_>>()
                .join(", "),
            path,
        }),
    }
}

fn resolve_qualified(
    line: usize,
    short: &str,
    doc: &Document,
    index: &HashMap<String, Vec<usize>>,
) -> Result<ResolvedLine, LinehashError> {
    let path = doc.path.display().to_string();
    let idx = line
        .checked_sub(1)
        .ok_or_else(|| LinehashError::InvalidAnchor {
            anchor: format!("{line}:{short}"),
        })?;

    let actual = doc
        .lines
        .get(idx)
        .ok_or_else(|| LinehashError::InvalidAnchor {
            anchor: format!("{line}:{short}"),
        })?;

    if actual.short_hash == short {
        return Ok(ResolvedLine {
            index: idx,
            line_no: line,
            short_hash: short.to_owned(),
        });
    }

    let _exists_elsewhere = index.get(short);
    Err(LinehashError::StaleAnchor {
        anchor: format!("{line}:{short}"),
        line,
        expected: short.to_owned(),
        actual: actual.short_hash.clone(),
        path,
    })
}

fn normalize_anchor_input(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn validate_short_hash(short: &str, original: &str) -> Result<(), LinehashError> {
    let is_valid = short.len() == 2 && short.chars().all(|ch| ch.is_ascii_hexdigit());
    if is_valid {
        Ok(())
    } else {
        Err(LinehashError::InvalidAnchor {
            anchor: original.trim().to_owned(),
        })
    }
}

fn parse_line_number(raw: &str, original: &str) -> Result<usize, LinehashError> {
    let line = raw
        .parse::<usize>()
        .map_err(|_| LinehashError::InvalidAnchor {
            anchor: original.trim().to_owned(),
        })?;

    if line == 0 {
        return Err(LinehashError::InvalidAnchor {
            anchor: original.trim().to_owned(),
        });
    }

    Ok(line)
}

fn display_anchor(anchor: &Anchor) -> String {
    match anchor {
        Anchor::Hash { short } => short.clone(),
        Anchor::LineHash { line, short } => format!("{line}:{short}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Anchor, ResolvedLine, parse_anchor, parse_range, resolve, resolve_all, resolve_range,
    };
    use crate::document::Document;
    use crate::error::LinehashError;
    use std::path::Path;

    #[test]
    fn test_parse_unqualified_lowercase() {
        assert_eq!(
            parse_anchor("f1").unwrap(),
            Anchor::Hash { short: "f1".into() }
        );
    }

    #[test]
    fn test_parse_unqualified_uppercase_normalizes() {
        assert_eq!(
            parse_anchor("F1").unwrap(),
            Anchor::Hash { short: "f1".into() }
        );
    }

    #[test]
    fn test_parse_qualified_basic() {
        assert_eq!(
            parse_anchor("2:f1").unwrap(),
            Anchor::LineHash {
                line: 2,
                short: "f1".into()
            }
        );
    }

    #[test]
    fn test_parse_qualified_uppercase_normalizes() {
        assert_eq!(
            parse_anchor("2:F1").unwrap(),
            Anchor::LineHash {
                line: 2,
                short: "f1".into()
            }
        );
    }

    #[test]
    fn test_parse_range_basic() {
        let range = parse_range("2:f1..4:9c").unwrap();
        assert_eq!(
            range.start,
            Anchor::LineHash {
                line: 2,
                short: "f1".into()
            }
        );
        assert_eq!(
            range.end,
            Anchor::LineHash {
                line: 4,
                short: "9c".into()
            }
        );
    }

    #[test]
    fn test_parse_invalid_hash_length_3_chars_fails() {
        assert!(matches!(
            parse_anchor("abc"),
            Err(LinehashError::InvalidAnchor { .. })
        ));
    }

    #[test]
    fn test_parse_invalid_hash_non_hex_fails() {
        assert!(matches!(
            parse_anchor("zz"),
            Err(LinehashError::InvalidAnchor { .. })
        ));
    }

    #[test]
    fn test_parse_line_number_zero_fails() {
        assert!(matches!(
            parse_anchor("0:aa"),
            Err(LinehashError::InvalidAnchor { .. })
        ));
    }

    #[test]
    fn test_parse_line_number_negative_fails() {
        assert!(matches!(
            parse_anchor("-1:aa"),
            Err(LinehashError::InvalidAnchor { .. })
        ));
    }

    #[test]
    fn test_resolve_unqualified_not_found() {
        let doc = sample_doc();
        let index = doc.build_index();
        let error = resolve(&Anchor::Hash { short: "ff".into() }, &doc, &index).unwrap_err();

        assert!(matches!(error, LinehashError::HashNotFound { .. }));
    }

    #[test]
    fn test_resolve_unqualified_single_match() {
        let doc = sample_doc();
        let index = doc.build_index();
        let short = doc.lines[1].short_hash.clone();

        assert_eq!(
            resolve(
                &Anchor::Hash {
                    short: short.clone()
                },
                &doc,
                &index
            )
            .unwrap(),
            ResolvedLine {
                index: 1,
                line_no: 2,
                short_hash: short
            }
        );
    }

    #[test]
    fn test_resolve_unqualified_ambiguous() {
        let doc = collision_doc();
        let index = doc.build_index();
        let short = doc.lines[0].short_hash.clone();
        let error = resolve(&Anchor::Hash { short }, &doc, &index).unwrap_err();

        assert!(matches!(error, LinehashError::AmbiguousHash { .. }));
    }

    #[test]
    fn test_resolve_qualified_match() {
        let doc = sample_doc();
        let index = doc.build_index();
        let short = doc.lines[1].short_hash.clone();

        assert_eq!(
            resolve(
                &Anchor::LineHash {
                    line: 2,
                    short: short.clone()
                },
                &doc,
                &index
            )
            .unwrap(),
            ResolvedLine {
                index: 1,
                line_no: 2,
                short_hash: short
            }
        );
    }

    #[test]
    fn test_resolve_qualified_stale() {
        let doc = sample_doc();
        let index = doc.build_index();
        let error = resolve(
            &Anchor::LineHash {
                line: 2,
                short: "ff".into(),
            },
            &doc,
            &index,
        )
        .unwrap_err();

        assert!(matches!(error, LinehashError::StaleAnchor { .. }));
    }

    #[test]
    fn test_resolve_qualified_out_of_range_line() {
        let doc = sample_doc();
        let index = doc.build_index();
        let error = resolve(
            &Anchor::LineHash {
                line: 99,
                short: "aa".into(),
            },
            &doc,
            &index,
        )
        .unwrap_err();

        assert!(matches!(error, LinehashError::InvalidAnchor { .. }));
    }

    #[test]
    fn test_resolve_range_valid() {
        let doc = sample_doc();
        let index = doc.build_index();
        let start = format!("1:{}", doc.lines[0].short_hash);
        let end = format!("3:{}", doc.lines[2].short_hash);
        let range = parse_range(&format!("{start}..{end}")).unwrap();

        let (resolved_start, resolved_end) = resolve_range(&range, &doc, &index).unwrap();
        assert_eq!(resolved_start.index, 0);
        assert_eq!(resolved_end.index, 2);
    }

    #[test]
    fn test_resolve_range_start_after_end_fails() {
        let doc = sample_doc();
        let index = doc.build_index();
        let start = format!("3:{}", doc.lines[2].short_hash);
        let end = format!("1:{}", doc.lines[0].short_hash);
        let range = parse_range(&format!("{start}..{end}")).unwrap();

        let error = resolve_range(&range, &doc, &index).unwrap_err();
        assert!(matches!(error, LinehashError::InvalidRange { .. }));
    }

    #[test]
    fn test_resolve_all_collects_all_errors() {
        let doc = collision_doc();
        let index = doc.build_index();
        let results = resolve_all(
            &[
                Anchor::Hash {
                    short: doc.lines[0].short_hash.clone(),
                },
                Anchor::Hash { short: "ff".into() },
            ],
            &doc,
            &index,
        );

        assert!(matches!(
            results[0],
            Err(LinehashError::AmbiguousHash { .. })
        ));
        assert!(matches!(
            results[1],
            Err(LinehashError::HashNotFound { .. })
        ));
    }

    #[test]
    fn test_resolve_all_all_success() {
        let doc = sample_doc();
        let index = doc.build_index();
        let results = resolve_all(
            &[
                Anchor::LineHash {
                    line: 1,
                    short: doc.lines[0].short_hash.clone(),
                },
                Anchor::LineHash {
                    line: 2,
                    short: doc.lines[1].short_hash.clone(),
                },
            ],
            &doc,
            &index,
        );

        assert!(results.iter().all(|result| result.is_ok()));
    }

    fn sample_doc() -> Document {
        Document::from_str(Path::new("demo.txt"), "alpha\nbeta\ngamma\n").unwrap()
    }

    fn collision_doc() -> Document {
        for i in 0..10_000 {
            let left = format!("line-{i}");
            for j in (i + 1)..10_000 {
                let right = format!("line-{j}");
                let doc = Document::from_str(Path::new("demo.txt"), &format!("{left}\n{right}\n"))
                    .unwrap();
                if doc.lines[0].short_hash == doc.lines[1].short_hash {
                    return doc;
                }
            }
        }
        panic!("failed to find a collision doc");
    }
}
