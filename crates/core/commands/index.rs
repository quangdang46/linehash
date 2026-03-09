use std::io::Write;

use crate::cli::IndexCmd;
use crate::context::CommandContext;
use crate::document::Document;
use crate::error::LinehashError;
use crate::output;

pub fn run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    cmd: IndexCmd,
) -> Result<(), LinehashError> {
    let doc = Document::load(&cmd.file)?;

    if cmd.json {
        output::print_index_json(ctx.stdout(), &doc)?;
    } else {
        output::print_index(ctx.stdout(), &doc)?;
    }

    Ok(())
}
