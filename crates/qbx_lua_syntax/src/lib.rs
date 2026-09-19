pub mod ast;
pub mod lexer;
mod parser;
pub mod span;
pub mod visit;

pub use lexer::{Comment, CommentKind, NumberValue, Token, TokenKind};
pub use parser::parse;
pub use smol_str::SmolStr;
pub use span::{LineCol, LineIndex, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub span: Span,
    pub message: String,
}
