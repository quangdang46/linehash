#![allow(dead_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LinehashError {
    #[error("{command} is not implemented yet")]
    NotImplemented { command: &'static str },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("file '{path}' is not valid UTF-8")]
    InvalidUtf8 { path: String },

    #[error("file '{path}' appears to be binary and cannot be edited safely")]
    BinaryFile { path: String },

    #[error("file '{path}' uses mixed newline styles")]
    MixedNewlines { path: String },

    #[error("invalid anchor '{anchor}'")]
    InvalidAnchor { anchor: String },

    #[error("invalid range anchor '{range}'")]
    InvalidRange { range: String },

    #[error("hash '{hash}' not found in {path}")]
    HashNotFound { hash: String, path: String },

    #[error("hash '{hash}' matches {count} lines in {path} (lines {lines})")]
    AmbiguousHash {
        hash: String,
        count: usize,
        lines: String,
        path: String,
    },

    #[error(
        "line {line} content changed since last read in {path} (expected hash {expected}, got {actual})"
    )]
    StaleAnchor {
        anchor: String,
        line: usize,
        expected: String,
        actual: String,
        path: String,
    },

    #[error("file '{path}' changed since the last read")]
    StaleFile { path: String },

    #[error("invalid pattern '{pattern}': {message}")]
    InvalidPattern { pattern: String, message: String },

    #[error("patch failed at operation {op_index}: {reason}")]
    PatchFailed { op_index: usize, reason: String },

    #[error("multi-line content is not supported in v1")]
    MultiLineContentUnsupported,

    #[error("mutation index {index} is out of bounds for document with {len} lines")]
    MutationIndexOutOfBounds { index: usize, len: usize },

    #[error("mutation range {start}..={end} is invalid for document with {len} lines")]
    InvalidMutationRange {
        start: usize,
        end: usize,
        len: usize,
    },
}

impl LinehashError {
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            LinehashError::NotImplemented { .. } => {
                Some("continue with the next planned implementation bead")
            }
            LinehashError::InvalidUtf8 { .. } => {
                Some("convert the file to UTF-8 before using linehash")
            }
            LinehashError::BinaryFile { .. } => Some("linehash only supports UTF-8 text files"),
            LinehashError::MixedNewlines { .. } => {
                Some("run `dos2unix <file>` or `unix2dos <file>` to normalize first")
            }
            LinehashError::InvalidAnchor { .. } => {
                Some("use a 2-char hash like 'f1' or a qualified anchor like '2:f1'")
            }
            LinehashError::InvalidRange { .. } => Some("use a range like '2:f1..4:9c'"),
            LinehashError::HashNotFound { .. } => {
                Some("run `linehash read <file>` to get current hashes")
            }
            LinehashError::AmbiguousHash { .. } => {
                Some("use a line-qualified hash like '2:f1' to disambiguate")
            }
            LinehashError::StaleAnchor { .. } => {
                Some("re-read the file with `linehash read <file>` and retry the edit")
            }
            LinehashError::StaleFile { .. } => Some(
                "re-read the file metadata and retry with fresh --expect-mtime/--expect-inode values",
            ),
            LinehashError::InvalidPattern { .. } => Some("fix the pattern syntax and try again"),
            LinehashError::PatchFailed { .. } => {
                Some("fix the failing patch operation and retry the transaction")
            }
            LinehashError::MultiLineContentUnsupported => {
                Some("use `linehash patch` with multiple ops for multi-line replacement")
            }
            LinehashError::MutationIndexOutOfBounds { .. } => {
                Some("re-check the resolved line number against the current document and retry")
            }
            LinehashError::InvalidMutationRange { .. } => {
                Some("use a valid in-bounds range where the start line is not after the end line")
            }
            LinehashError::Io(_) => {
                Some("check the file path and permissions, then retry the command")
            }
            LinehashError::Json(_) => {
                Some("fix the JSON input or output handling and retry the command")
            }
        }
    }

    pub fn command(&self) -> Option<&'static str> {
        match self {
            LinehashError::NotImplemented { command } => Some(command),
            LinehashError::Io(_)
            | LinehashError::Json(_)
            | LinehashError::InvalidUtf8 { .. }
            | LinehashError::BinaryFile { .. }
            | LinehashError::MixedNewlines { .. }
            | LinehashError::InvalidAnchor { .. }
            | LinehashError::InvalidRange { .. }
            | LinehashError::HashNotFound { .. }
            | LinehashError::AmbiguousHash { .. }
            | LinehashError::StaleAnchor { .. }
            | LinehashError::StaleFile { .. }
            | LinehashError::InvalidPattern { .. }
            | LinehashError::PatchFailed { .. }
            | LinehashError::MultiLineContentUnsupported
            | LinehashError::MutationIndexOutOfBounds { .. }
            | LinehashError::InvalidMutationRange { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LinehashError;

    #[test]
    fn every_error_variant_has_a_recovery_hint() {
        let errors = vec![
            LinehashError::NotImplemented { command: "patch" },
            LinehashError::Io(std::io::Error::other("boom")),
            LinehashError::Json(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
            LinehashError::InvalidUtf8 {
                path: "demo.txt".into(),
            },
            LinehashError::BinaryFile {
                path: "demo.bin".into(),
            },
            LinehashError::MixedNewlines {
                path: "demo.txt".into(),
            },
            LinehashError::InvalidAnchor {
                anchor: "bogus".into(),
            },
            LinehashError::InvalidRange {
                range: "1:aa..0:bb".into(),
            },
            LinehashError::HashNotFound {
                hash: "ff".into(),
                path: "demo.txt".into(),
            },
            LinehashError::AmbiguousHash {
                hash: "aa".into(),
                count: 2,
                lines: "1, 3".into(),
                path: "demo.txt".into(),
            },
            LinehashError::StaleAnchor {
                anchor: "2:aa".into(),
                line: 2,
                expected: "aa".into(),
                actual: "bb".into(),
                path: "demo.txt".into(),
            },
            LinehashError::StaleFile {
                path: "demo.txt".into(),
            },
            LinehashError::InvalidPattern {
                pattern: "(".into(),
                message: "unclosed group".into(),
            },
            LinehashError::PatchFailed {
                op_index: 1,
                reason: "bad op".into(),
            },
            LinehashError::MultiLineContentUnsupported,
            LinehashError::MutationIndexOutOfBounds { index: 5, len: 2 },
            LinehashError::InvalidMutationRange {
                start: 3,
                end: 1,
                len: 2,
            },
        ];

        for error in errors {
            assert!(
                error.hint().is_some(),
                "expected a recovery hint for error variant: {error:?}"
            );
        }
    }

    #[test]
    fn not_implemented_reports_command_name() {
        let error = LinehashError::NotImplemented { command: "patch" };
        assert_eq!(error.command(), Some("patch"));
    }
}
