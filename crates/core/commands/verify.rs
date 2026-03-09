use std::io::Write;

use crate::cli::VerifyCmd;
use crate::context::CommandContext;
use crate::error::LinehashError;

pub fn run<W: Write, E: Write>(
    _ctx: &mut CommandContext<'_, W, E>,
    _cmd: VerifyCmd,
) -> Result<(), LinehashError> {
    Err(LinehashError::NotImplemented { command: "verify" })
}
