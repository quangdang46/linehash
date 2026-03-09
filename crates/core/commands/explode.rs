use std::io::Write;

use crate::cli::ExplodeCmd;
use crate::context::CommandContext;
use crate::error::LinehashError;

pub fn run<W: Write, E: Write>(
    _ctx: &mut CommandContext<'_, W, E>,
    _cmd: ExplodeCmd,
) -> Result<(), LinehashError> {
    Err(LinehashError::NotImplemented { command: "explode" })
}
