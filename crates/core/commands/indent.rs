use std::io::Write;

use crate::cli::IndentCmd;
use crate::context::CommandContext;
use crate::error::LinehashError;

pub fn run<W: Write, E: Write>(
    _ctx: &mut CommandContext<'_, W, E>,
    _cmd: IndentCmd,
) -> Result<(), LinehashError> {
    Err(LinehashError::NotImplemented { command: "indent" })
}
