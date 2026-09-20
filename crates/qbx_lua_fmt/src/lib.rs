mod printer;
mod verify;

use qbx_lua_syntax::parse;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum QuoteStyle {
    #[default]
    Preserve,
    Single,
    Double,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FormatOptions {
    pub indent_width: usize,
    pub use_tabs: bool,
    pub line_width: usize,
    pub quote_style: QuoteStyle,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { indent_width: 4, use_tabs: false, line_width: 120, quote_style: QuoteStyle::Preserve }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The file does not parse; formatting it could move the error around or lose code.
    SyntaxError { line: u32, message: String },
    /// The formatted text no longer lexes to the same program. This is a formatter bug, and the
    /// original text is kept.
    Unsafe(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::SyntaxError { line, message } => write!(f, "syntax error on line {line}: {message}"),
            FormatError::Unsafe(reason) => write!(f, "refusing to format: {reason}"),
        }
    }
}

/// Formats `source`. The result is re-lexed and compared with the input, so a successful return
/// is guaranteed to contain the same tokens and comments.
pub fn format(source: &str, options: &FormatOptions) -> Result<String, FormatError> {
    let uses_crlf = source.contains("\r\n");
    let normalised = if uses_crlf { source.replace("\r\n", "\n") } else { source.to_string() };
    let chunk = parse(&normalised);
    if let Some(error) = chunk.errors.first() {
        let line = normalised[..error.span.start as usize].matches('\n').count() as u32 + 1;
        return Err(FormatError::SyntaxError { line, message: error.message.clone() });
    }
    let mut formatted = printer::print(&normalised, &chunk, options);
    if uses_crlf {
        formatted = formatted.replace('\n', "\r\n");
    }
    verify::same_program(source, &formatted).map_err(FormatError::Unsafe)?;
    Ok(formatted)
}
