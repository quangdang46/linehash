use std::io::Write;

use crate::anchor::{parse_anchor, resolve};
use crate::cli::ReadCmd;
use crate::context::CommandContext;
use crate::document::Document;
use crate::error::LinehashError;
use crate::output;

pub fn run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    cmd: ReadCmd,
) -> Result<(), LinehashError> {
    let doc = Document::load(&cmd.file)?;

    if cmd.json {
        output::print_read_json(ctx.stdout(), &doc)?;
        return Ok(());
    }

    if cmd.anchor.is_empty() {
        output::print_read(ctx.stdout(), &doc)?;
        return Ok(());
    }

    let index = doc.build_index();
    let mut resolved = Vec::with_capacity(cmd.anchor.len());
    for anchor in &cmd.anchor {
        let parsed = parse_anchor(anchor)?;
        resolved.push(resolve(&parsed, &doc, &index)?);
    }

    output::print_read_context(ctx.stdout(), &doc, &resolved, cmd.context)?;
    Ok(())
}
