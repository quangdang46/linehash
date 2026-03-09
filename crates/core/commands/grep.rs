use std::io::Write;

use regex::RegexBuilder;

use crate::cli::GrepCmd;
use crate::context::CommandContext;
use crate::document::Document;
use crate::error::LinehashError;
use crate::output;

pub fn run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    cmd: GrepCmd,
) -> Result<(), LinehashError> {
    let doc = Document::load(&cmd.file)?;
    let regex = RegexBuilder::new(&cmd.pattern)
        .case_insensitive(cmd.case_insensitive)
        .build()
        .map_err(|error| LinehashError::InvalidPattern {
            pattern: cmd.pattern.clone(),
            message: error.to_string(),
        })?;

    let indexes = doc
        .lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let is_match = regex.is_match(&line.content);
            let include = if cmd.invert { !is_match } else { is_match };
            include.then_some(index)
        })
        .collect::<Vec<_>>();

    if cmd.json {
        output::write_grep_json(ctx, &doc, &indexes)?;
    } else {
        output::print_grep(ctx.stdout(), &doc, &indexes)?;
    }

    Ok(())
}
