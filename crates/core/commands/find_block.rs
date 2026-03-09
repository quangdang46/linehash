use std::io::Write;

use crate::cli::FindBlockCmd;
use crate::context::CommandContext;
use crate::error::LinehashError;

pub fn run<W: Write, E: Write>(
    _ctx: &mut CommandContext<'_, W, E>,
    _cmd: FindBlockCmd,
) -> Result<(), LinehashError> {
    Err(LinehashError::NotImplemented {
        command: "find-block",
    })
}
