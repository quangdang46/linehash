use std::io::Write;

use serde::Serialize;

use crate::anchor::{parse_anchor, resolve};
use crate::cli::VerifyCmd;
use crate::context::CommandContext;
use crate::document::Document;
use crate::error::LinehashError;
use crate::output;

#[derive(Serialize)]
struct VerifyResult {
    anchor: String,
    status: &'static str,
    line_no: Option<usize>,
    content: Option<String>,
    error: Option<String>,
}

pub fn run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    cmd: VerifyCmd,
) -> Result<i32, LinehashError> {
    let doc = Document::load(&cmd.file)?;
    let index = doc.build_index();
    let mut results = Vec::with_capacity(cmd.anchors.len());
    let mut has_failures = false;

    for anchor_str in cmd.anchors {
        match parse_anchor(&anchor_str) {
            Ok(anchor) => match resolve(&anchor, &doc, &index) {
                Ok(resolved) => {
                    let content = doc.lines[resolved.index].content.clone();
                    results.push(VerifyResult {
                        anchor: anchor_str,
                        status: "ok",
                        line_no: Some(resolved.line_no),
                        content: Some(content),
                        error: None,
                    });
                }
                Err(error) => {
                    has_failures = true;
                    results.push(VerifyResult {
                        anchor: anchor_str,
                        status: status_for_error(&error),
                        line_no: line_no_for_error(&error),
                        content: None,
                        error: Some(error.to_string()),
                    });
                }
            },
            Err(error) => {
                has_failures = true;
                results.push(VerifyResult {
                    anchor: anchor_str,
                    status: status_for_error(&error),
                    line_no: None,
                    content: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    if cmd.json {
        output::write_json_success(ctx, &results)?;
    } else {
        for result in &results {
            match result.status {
                "ok" => output::write_success_line(
                    ctx,
                    &format!(
                        "✓  {}  resolves → {:?}",
                        result.anchor,
                        result.content.as_deref().unwrap_or("")
                    ),
                )?,
                _ => output::write_success_line(
                    ctx,
                    &format!(
                        "✗  {}  {}",
                        result.anchor,
                        result.error.as_deref().unwrap_or("unknown error")
                    ),
                )?,
            }
        }
    }

    Ok(if has_failures { 1 } else { 0 })
}

fn status_for_error(error: &LinehashError) -> &'static str {
    match error {
        LinehashError::HashNotFound { .. } => "not_found",
        LinehashError::AmbiguousHash { .. } => "ambiguous",
        LinehashError::StaleAnchor { .. } => "stale",
        LinehashError::InvalidAnchor { .. } => "parse_error",
        _ => "error",
    }
}

fn line_no_for_error(error: &LinehashError) -> Option<usize> {
    match error {
        LinehashError::StaleAnchor { line, .. } => Some(*line),
        _ => None,
    }
}
