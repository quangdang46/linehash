use std::io::Write;

use crate::anchor::{parse_anchor, resolve};
use crate::cli::DeleteCmd;
use crate::commands::common::{atomic_write, check_guard};
use crate::context::{CommandContext, OutputMode};
use crate::document::Document;
use crate::error::LinehashError;
use crate::mutation::delete_line;
use crate::output;

pub fn run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    cmd: DeleteCmd,
) -> Result<(), LinehashError> {
    let mut doc = Document::load(&cmd.file)?;
    check_guard(&doc, cmd.expect_mtime, cmd.expect_inode)?;
    let index = doc.build_index();
    let anchor = parse_anchor(&cmd.anchor)?;
    let resolved = resolve(&anchor, &doc, &index)?;
    let deleted = doc.lines[resolved.index].content.clone();
    delete_line(&mut doc, resolved.index)?;

    let summary = DeleteSummary {
        line_no: resolved.line_no,
        deleted,
    };

    if cmd.dry_run {
        return write_dry_run(ctx, &doc, &summary);
    }

    atomic_write(&cmd.file, &doc.render())?;

    match ctx.output_mode() {
        OutputMode::Json => Ok(()),
        OutputMode::Pretty => {
            output::write_success_line(ctx, &summary.success_message()).map_err(LinehashError::from)
        }
    }
}

fn write_dry_run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    doc: &Document,
    summary: &DeleteSummary,
) -> Result<(), LinehashError> {
    match ctx.output_mode() {
        OutputMode::Json => output::print_read_json(ctx.stdout(), doc).map_err(LinehashError::from),
        OutputMode::Pretty => {
            output::write_success_line(ctx, &format!("Would delete line {}:", summary.line_no))?;
            output::write_success_line(ctx, &format!("  - {:?}", summary.deleted))?;
            output::write_success_line(ctx, "No file was written.").map_err(LinehashError::from)
        }
    }
}

struct DeleteSummary {
    line_no: usize,
    deleted: String,
}

impl DeleteSummary {
    fn success_message(&self) -> String {
        format!("Deleted line {}.", self.line_no)
    }
}
