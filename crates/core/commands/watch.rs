use std::io::Write;

use crate::cli::WatchCmd;
use crate::context::CommandContext;
use crate::error::LinehashError;

pub fn run<W: Write, E: Write>(
    _ctx: &mut CommandContext<'_, W, E>,
    _cmd: WatchCmd,
) -> Result<(), LinehashError> {
    Err(LinehashError::NotImplemented { command: "watch" })
}
