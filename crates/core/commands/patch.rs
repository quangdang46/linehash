use std::io::Write;

use crate::cli::PatchCmd;
use crate::context::CommandContext;
use crate::error::LinehashError;

pub fn run<W: Write, E: Write>(
    _ctx: &mut CommandContext<'_, W, E>,
    _cmd: PatchCmd,
) -> Result<(), LinehashError> {
    Err(LinehashError::NotImplemented { command: "patch" })
}
