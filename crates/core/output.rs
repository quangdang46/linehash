use std::io::{self, Write};

use serde::Serialize;

use crate::context::{CommandContext, OutputMode};
use crate::error::LinehashError;

#[derive(Serialize)]
struct ErrorPayload<'a> {
    error: String,
    hint: Option<&'a str>,
    command: Option<&'a str>,
}

#[allow(dead_code)]
pub fn write_success_line<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    line: &str,
) -> io::Result<()> {
    writeln!(ctx.stdout(), "{line}")
}

#[allow(dead_code)]
pub fn write_json_success<W: Write, E: Write, T: Serialize>(
    ctx: &mut CommandContext<'_, W, E>,
    value: &T,
) -> io::Result<()> {
    serde_json::to_writer_pretty(ctx.stdout(), value)?;
    writeln!(ctx.stdout())
}

pub fn write_error<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    error: &LinehashError,
) -> io::Result<()> {
    match ctx.output_mode() {
        OutputMode::Pretty => {
            writeln!(ctx.stderr(), "Error: {error}")?;
            if let Some(hint) = error.hint() {
                writeln!(ctx.stderr(), "Hint: {hint}")?;
            }
            Ok(())
        }
        OutputMode::Json => {
            let payload = ErrorPayload {
                error: error.to_string(),
                hint: error.hint(),
                command: error.command(),
            };
            serde_json::to_writer_pretty(ctx.stderr(), &payload)?;
            writeln!(ctx.stderr())
        }
    }
}
