use lsp_types::{Position, Range, TextEdit};
use qbx_lua_syntax::CommentKind;

use crate::document::Document;

/// Keeps a block of LuaCATS annotations going on Enter: after a `---@param ...` line the new line
/// starts with `---@`, and Enter on a line that holds only `---@` clears it again and stays there,
/// so the code typed next sits directly below its annotations.
pub fn on_type_formatting(doc: &Document, position: Position, ch: &str) -> Vec<TextEdit> {
    if ch != "\n" || position.line == 0 || position.line >= doc.lines.line_count() {
        return Vec::new();
    }
    let cursor = doc.offset(position) as usize;
    let line = doc.lines.line_span(position.line);
    let typed = doc.text.get(line.start as usize..cursor).unwrap_or_default().trim();
    let rest = doc.text.get(cursor..line.end as usize).unwrap_or_default();
    // Enter in the middle of a line moves text down, which is left alone. Editors that continue
    // `---` comments themselves leave a bare `---` on the new line.
    if !rest.trim().is_empty() || !matches!(typed, "" | "---") {
        return Vec::new();
    }
    let previous_span = doc.lines.line_span(position.line - 1);
    let previous = doc.text[previous_span.start as usize..previous_span.end as usize].trim_end_matches(['\r', '\n']);
    let content = previous.trim_start();
    let indent = &previous[..previous.len() - content.len()];
    let Some(tag) = content.strip_prefix("---@") else { return Vec::new() };
    // The `---` has to open a comment, not sit inside a long string.
    let comment_start = previous_span.start + indent.len() as u32;
    if !doc.chunk.comments.iter().any(|c| c.span.start == comment_start && c.kind == CommentKind::Line) {
        return Vec::new();
    }
    if tag.trim().is_empty() {
        return vec![TextEdit::new(Range::new(doc.position(comment_start), position), String::new())];
    }
    vec![TextEdit::new(Range::new(Position::new(position.line, 0), position), format!("{indent}---@"))]
}
