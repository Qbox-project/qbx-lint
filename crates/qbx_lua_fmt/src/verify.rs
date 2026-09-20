use qbx_lua_syntax::lexer::{decode_string, lex, Lexed};
use qbx_lua_syntax::{Token, TokenKind};

/// Statement separators and the separator after the last table field carry no meaning, and the
/// printer normalises both.
fn significant(lexed: &Lexed) -> Vec<Token> {
    let tokens = &lexed.tokens;
    let is_separator = |kind| matches!(kind, TokenKind::Semi | TokenKind::Comma);
    tokens
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            let before_brace = tokens.get(i + 1).is_some_and(|next| next.kind == TokenKind::RBrace);
            !(t.kind == TokenKind::Semi && !before_brace) && !(is_separator(t.kind) && before_brace)
        })
        .map(|(_, t)| *t)
        .collect()
}

fn token_value(source: &str, token: Token) -> String {
    let text = token.span.text(source);
    match token.kind {
        TokenKind::String => decode_string(text, 0).value,
        TokenKind::LongString => decode_string(text, 0).value,
        TokenKind::Name | TokenKind::Number | TokenKind::JenkinsHash | TokenKind::Unknown => text.to_string(),
        _ => String::new(),
    }
}

fn line_of(source: &str, offset: u32) -> usize {
    source[..offset as usize].matches('\n').count() + 1
}

pub fn same_program(original: &str, formatted: &str) -> Result<(), String> {
    let (before, after) = (lex(original), lex(formatted));
    if !after.errors.is_empty() {
        return Err("the formatted text does not lex".into());
    }
    let (a, b) = (significant(&before), significant(&after));
    for (x, y) in a.iter().zip(&b) {
        if x.kind != y.kind || token_value(original, *x) != token_value(formatted, *y) {
            return Err(format!("token mismatch near line {}", line_of(original, x.span.start)));
        }
    }
    if a.len() != b.len() {
        return Err(format!("token count changed from {} to {}", a.len(), b.len()));
    }
    if before.comments.len() != after.comments.len() {
        return Err(format!("comment count changed from {} to {}", before.comments.len(), after.comments.len()));
    }
    for (x, y) in before.comments.iter().zip(&after.comments) {
        let normalise = |text: &str| text.replace("\r\n", "\n").trim_end().to_string();
        if normalise(x.span.text(original)) != normalise(y.span.text(formatted)) {
            return Err(format!("comment changed near line {}", line_of(original, x.span.start)));
        }
    }
    Ok(())
}
