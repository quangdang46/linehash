use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::anchor::{parse_anchor, resolve};
use crate::cli::FindBlockCmd;
use crate::context::{CommandContext, OutputMode};
use crate::document::Document;
use crate::error::LinehashError;
use crate::output;

pub fn run<W: Write, E: Write>(
    ctx: &mut CommandContext<'_, W, E>,
    cmd: FindBlockCmd,
) -> Result<(), LinehashError> {
    let doc = Document::load(&cmd.file)?;
    let index = doc.build_index();
    let anchor = parse_anchor(&cmd.anchor)?;
    let resolved = resolve(&anchor, &doc, &index)?;
    let language = detect_language(&doc, resolved.index)?;
    let block = match language {
        BlockLanguage::Brace => find_brace_block(&doc, resolved.index)?,
        BlockLanguage::Indent => find_indent_block(&doc, resolved.index)?,
    };

    match ctx.output_mode() {
        OutputMode::Pretty => output::write_success_line(
            ctx,
            &format!(
                "Block: {}:{}..{}:{}  ({} lines — {})",
                block.start_line,
                crate::document::format_short_hash(doc.lines[block.start_index].short_hash),
                block.end_line,
                crate::document::format_short_hash(doc.lines[block.end_index].short_hash),
                block.line_count(),
                language.description(),
            ),
        )
        .map_err(LinehashError::from),
        OutputMode::Json => output::write_json_success(
            ctx,
            &BlockPayload {
                start: format!(
                    "{}:{}",
                    block.start_line,
                    crate::document::format_short_hash(doc.lines[block.start_index].short_hash)
                ),
                end: format!(
                    "{}:{}",
                    block.end_line,
                    crate::document::format_short_hash(doc.lines[block.end_index].short_hash)
                ),
                line_count: block.line_count(),
                language: language.name(),
            },
        )
        .map_err(LinehashError::from),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlockLanguage {
    Brace,
    Indent,
}

impl BlockLanguage {
    fn name(self) -> &'static str {
        match self {
            BlockLanguage::Brace => "brace",
            BlockLanguage::Indent => "indent",
        }
    }

    fn description(self) -> &'static str {
        match self {
            BlockLanguage::Brace => "brace-balanced",
            BlockLanguage::Indent => "indent-delimited",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BlockRange {
    start_index: usize,
    end_index: usize,
    start_line: usize,
    end_line: usize,
}

impl BlockRange {
    fn line_count(self) -> usize {
        self.end_line - self.start_line + 1
    }
}

#[derive(Serialize)]
struct BlockPayload {
    start: String,
    end: String,
    line_count: usize,
    language: &'static str,
}

fn detect_language(doc: &Document, anchor_index: usize) -> Result<BlockLanguage, LinehashError> {
    if is_indent_extension(&doc.path) {
        return Ok(BlockLanguage::Indent);
    }
    if is_brace_extension(&doc.path) {
        return Ok(BlockLanguage::Brace);
    }

    let mut saw_brace = false;
    let mut saw_indent = false;
    for index in 0..doc.lines.len() {
        let line = doc.lines[index].content.as_str();
        if line.contains('{') || line.contains('}') {
            saw_brace = true;
        }
        if looks_like_indent_block_header(line)
            && next_nonblank_indent(doc, index)
                .is_some_and(|next| next > leading_indent_width(line))
        {
            saw_indent = true;
        }
    }

    match (saw_brace, saw_indent) {
        (true, false) => Ok(BlockLanguage::Brace),
        (false, true) => Ok(BlockLanguage::Indent),
        _ => Err(LinehashError::AmbiguousBlockLanguage {
            line_no: anchor_index + 1,
        }),
    }
}

fn find_brace_block(doc: &Document, anchor_index: usize) -> Result<BlockRange, LinehashError> {
    let mut stack: Vec<usize> = Vec::new();
    let mut blocks = Vec::new();

    for (line_index, line) in doc.lines.iter().enumerate() {
        for ch in line.content.chars() {
            match ch {
                '{' => stack.push(line_index),
                '}' => {
                    let Some(start_index) = stack.pop() else {
                        return Err(LinehashError::UnbalancedBlock {
                            line_no: anchor_index + 1,
                        });
                    };
                    blocks.push(BlockRange {
                        start_index,
                        end_index: line_index,
                        start_line: start_index + 1,
                        end_line: line_index + 1,
                    });
                }
                _ => {}
            }
        }
    }

    if !stack.is_empty() {
        return Err(LinehashError::UnbalancedBlock {
            line_no: anchor_index + 1,
        });
    }

    blocks
        .into_iter()
        .filter(|block| block.start_index <= anchor_index && anchor_index <= block.end_index)
        .min_by_key(|block| (block.start_index, usize::MAX - block.end_index))
        .ok_or(LinehashError::UnbalancedBlock {
            line_no: anchor_index + 1,
        })
}

fn find_indent_block(doc: &Document, anchor_index: usize) -> Result<BlockRange, LinehashError> {
    let mut candidate = None;
    for index in 0..=anchor_index {
        let line = doc.lines[index].content.as_str();
        if is_blank(line) || !looks_like_indent_block_header(line) {
            continue;
        }
        let header_indent = leading_indent_width(line);
        let Some(next_indent) = next_nonblank_indent(doc, index) else {
            continue;
        };
        if next_indent <= header_indent {
            continue;
        }
        let end_index = indent_block_end(doc, index, header_indent);
        if candidate.is_none() && index <= anchor_index && anchor_index <= end_index {
            candidate = Some(BlockRange {
                start_index: index,
                end_index,
                start_line: index + 1,
                end_line: end_index + 1,
            });
        }
    }

    candidate.ok_or(LinehashError::UnbalancedBlock {
        line_no: anchor_index + 1,
    })
}

fn indent_block_end(doc: &Document, header_index: usize, header_indent: usize) -> usize {
    let mut end_index = header_index;
    for index in header_index + 1..doc.lines.len() {
        let line = doc.lines[index].content.as_str();
        if is_blank(line) {
            end_index = index;
            continue;
        }
        if leading_indent_width(line) <= header_indent {
            break;
        }
        end_index = index;
    }
    end_index
}

fn next_nonblank_indent(doc: &Document, from_index: usize) -> Option<usize> {
    doc.lines
        .iter()
        .skip(from_index + 1)
        .map(|line| line.content.as_str())
        .find(|line| !is_blank(line))
        .map(leading_indent_width)
}

fn leading_indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .count()
}

fn looks_like_indent_block_header(line: &str) -> bool {
    line.trim_end().ends_with(':')
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn is_indent_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("py" | "yaml" | "yml")
    )
}

fn is_brace_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "js" | "ts" | "jsx" | "tsx" | "java" | "c" | "cc" | "cpp" | "h" | "hpp" | "go")
    )
}
