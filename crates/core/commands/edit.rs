use std::io::Write;

use tempfile::NamedTempFile;

use crate::anchor::{parse_anchor, parse_range, resolve, resolve_range};
use crate::cli::EditCmd;
use crate::context::{CommandContext, OutputMode};
use crate::document::Document;
use crate::error::LinehashError;
use crate::mutation::{replace_line, replace_range_with_line};
use crate::output;

pub fn run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    cmd: EditCmd,
) -> Result<(), LinehashError> {
    let mut doc = Document::load(&cmd.file)?;
    check_guard(&doc, cmd.expect_mtime, cmd.expect_inode)?;
    let index = doc.build_index();

    let summary = match parse_range(&cmd.anchor) {
        Ok(range) => {
            let (start, end) = resolve_range(&range, &doc, &index)?;
            let before = doc.lines[start.index..=end.index]
                .iter()
                .map(|line| line.content.clone())
                .collect::<Vec<_>>();
            replace_range_with_line(&mut doc, start.index, end.index, &cmd.content)?;
            EditSummary::Range {
                start_line: start.line_no,
                end_line: end.line_no,
                before,
                after: cmd.content,
            }
        }
        Err(_) => {
            let anchor = parse_anchor(&cmd.anchor)?;
            let resolved = resolve(&anchor, &doc, &index)?;
            let before = doc.lines[resolved.index].content.clone();
            replace_line(&mut doc, resolved.index, &cmd.content)?;
            EditSummary::Single {
                line_no: resolved.line_no,
                before,
                after: cmd.content,
            }
        }
    };

    if cmd.dry_run {
        return write_dry_run(ctx, &doc, &summary);
    }

    atomic_write(&cmd.file, &doc.render())?;

    match ctx.output_mode() {
        OutputMode::Json => Ok(()),
        OutputMode::Pretty => output::write_success_line(ctx, &summary.success_message()).map_err(LinehashError::from),
    }
}

fn check_guard(
    doc: &Document,
    expect_mtime: Option<i64>,
    expect_inode: Option<u64>,
) -> Result<(), LinehashError> {
    let Some(meta) = &doc.file_meta else {
        return Ok(());
    };

    if expect_mtime.is_some_and(|expected| expected != meta.mtime_secs)
        || expect_inode.is_some_and(|expected| expected != meta.inode)
    {
        return Err(LinehashError::StaleFile {
            path: doc.path.display().to_string(),
        });
    }

    Ok(())
}

fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<(), LinehashError> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.flush()?;
    temp.persist(path)
        .map(|_| ())
        .map_err(|error| LinehashError::Io(error.error))
}

fn write_dry_run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    doc: &Document,
    summary: &EditSummary,
) -> Result<(), LinehashError> {
    match ctx.output_mode() {
        OutputMode::Json => output::print_read_json(ctx.stdout(), doc).map_err(LinehashError::from),
        OutputMode::Pretty => {
            match summary {
                EditSummary::Single {
                    line_no,
                    before,
                    after,
                } => {
                    output::write_success_line(ctx, &format!("Would change line {line_no}:"))?;
                    output::write_success_line(ctx, &format!("  - {before:?}"))?;
                    output::write_success_line(ctx, &format!("  + {after:?}"))?;
                }
                EditSummary::Range {
                    start_line,
                    end_line,
                    before,
                    after,
                } => {
                    output::write_success_line(
                        ctx,
                        &format!("Would change lines {start_line}-{end_line}:"),
                    )?;
                    for line in before {
                        output::write_success_line(ctx, &format!("  - {line:?}"))?;
                    }
                    output::write_success_line(ctx, &format!("  + {after:?}"))?;
                }
            }
            output::write_success_line(ctx, "No file was written.").map_err(LinehashError::from)
        }
    }
}

enum EditSummary {
    Single {
        line_no: usize,
        before: String,
        after: String,
    },
    Range {
        start_line: usize,
        end_line: usize,
        before: Vec<String>,
        after: String,
    },
}

impl EditSummary {
    fn success_message(&self) -> String {
        match self {
            EditSummary::Single { line_no, .. } => format!("Edited line {line_no}."),
            EditSummary::Range {
                start_line,
                end_line,
                ..
            } => format!("Edited lines {start_line}-{end_line}."),
        }
    }
}
