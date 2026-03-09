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
            LinehashError::Io(_) | LinehashError::Json(_) => None,
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
            | LinehashError::PatchFailed { .. } => None,
        }
    }
}
