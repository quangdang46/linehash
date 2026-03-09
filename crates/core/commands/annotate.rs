use std::io::Write;

use regex::RegexBuilder;

use crate::cli::AnnotateCmd;
use crate::context::CommandContext;
use crate::document::Document;
use crate::error::LinehashError;
use crate::output;

pub fn run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    cmd: AnnotateCmd,
) -> Result<i32, LinehashError> {
    let doc = Document::load(&cmd.file)?;
    let matched = if cmd.regex {
        let regex = RegexBuilder::new(&cmd.query).build().map_err(|error| {
            LinehashError::InvalidPattern {
                pattern: cmd.query.clone(),
                message: error.to_string(),
            }
        })?;

        doc.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| regex.is_match(&line.content).then_some(index))
            .collect::<Vec<_>>()
    } else {
        doc.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.content.contains(&cmd.query).then_some(index))
            .collect::<Vec<_>>()
    };

    if cmd.expect_one && matched.len() > 1 {
        if cmd.json {
            output::write_grep_json(ctx, &doc, &matched)?;
        } else {
            output::write_success_line(
                ctx,
                &format!("annotate: expected 1 match, found {}", matched.len()),
            )?;
            output::print_grep(ctx.stdout(), &doc, &matched)?;
        }
        return Ok(1);
    }

    if cmd.json {
        output::write_grep_json(ctx, &doc, &matched)?;
    } else if matched.is_empty() {
        output::write_success_line(ctx, "No matches found.")?;
    } else {
        output::print_grep(ctx.stdout(), &doc, &matched)?;
    }

    Ok(0)
}
