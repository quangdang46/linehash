use std::io::Write;

use crate::cli::ReadCmd;
use crate::context::CommandContext;
use crate::error::LinehashError;

pub fn run<W: Write, E: Write>(
    _ctx: &mut CommandContext<'_, W, E>,
    _cmd: ReadCmd,
) -> Result<(), LinehashError> {
    Err(LinehashError::NotImplemented { command: "read" })
}
