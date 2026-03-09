use std::io::Write;

use crate::cli::InsertCmd;
use crate::context::CommandContext;
use crate::error::LinehashError;

pub fn run<W: Write, E: Write>(
    _ctx: &mut CommandContext<'_, W, E>,
    _cmd: InsertCmd,
) -> Result<(), LinehashError> {
    Err(LinehashError::NotImplemented { command: "insert" })
}
