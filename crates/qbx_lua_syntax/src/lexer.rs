use crate::span::Span;
use crate::SyntaxError;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TokenKind {
    Eof,
    Name,
    Number,
    String,
    LongString,
    JenkinsHash,

    And,
    Break,
    Do,
    Else,
    Elseif,
    End,
    False,
    For,
    Function,
    Goto,
    If,
    In,
    Local,
    Nil,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    True,
    Until,
    While,

    Plus,
    Minus,
    Star,
    Slash,
    DoubleSlash,
    Percent,
    Caret,
    Pound,
    Amp,
    Tilde,
    Pipe,
    Shl,
    Shr,
    Eq,
    Ne,
    Le,
    Ge,
    Lt,
    Gt,
    Assign,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    DoubleColon,
    Semi,
    Colon,
    Comma,
    Dot,
    Concat,
    Ellipsis,

    Question,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    DoubleSlashAssign,
    PercentAssign,
    CaretAssign,
    AmpAssign,
    PipeAssign,
    ShlAssign,
    ShrAssign,
    ConcatAssign,

    Unknown,
}

impl TokenKind {
    pub fn is_keyword(self) -> bool {
        use TokenKind::*;
        matches!(
            self,
            And | Break
                | Do
                | Else
                | Elseif
                | End
                | False
                | For
                | Function
                | Goto
                | If
                | In
                | Local
                | Nil
                | Not
                | Or
                | Repeat
                | Return
                | Then
                | True
                | Until
                | While
        )
    }

    pub fn describe(self) -> &'static str {
        use TokenKind::*;
        match self {
            Eof => "<eof>",
            Name => "<name>",
            Number => "<number>",
            String | LongString => "<string>",
            JenkinsHash => "<hash>",
            And => "and",
            Break => "break",
            Do => "do",
            Else => "else",
            Elseif => "elseif",
            End => "end",
            False => "false",
            For => "for",
            Function => "function",
            Goto => "goto",
            If => "if",
            In => "in",
            Local => "local",
            Nil => "nil",
            Not => "not",
            Or => "or",
            Repeat => "repeat",
            Return => "return",
            Then => "then",
            True => "true",
            Until => "until",
            While => "while",
            Plus => "+",
            Minus => "-",
            Star => "*",
            Slash => "/",
            DoubleSlash => "//",
            Percent => "%",
            Caret => "^",
            Pound => "#",
            Amp => "&",
            Tilde => "~",
            Pipe => "|",
            Shl => "<<",
            Shr => ">>",
            Eq => "==",
            Ne => "~=",
            Le => "<=",
            Ge => ">=",
            Lt => "<",
            Gt => ">",
            Assign => "=",
            LParen => "(",
            RParen => ")",
            LBrace => "{",
            RBrace => "}",
            LBracket => "[",
            RBracket => "]",
            DoubleColon => "::",
            Semi => ";",
            Colon => ":",
            Comma => ",",
            Dot => ".",
            Concat => "..",
            Ellipsis => "...",
            Question => "?",
            PlusAssign => "+=",
            MinusAssign => "-=",
            StarAssign => "*=",
            SlashAssign => "/=",
            DoubleSlashAssign => "//=",
            PercentAssign => "%=",
            CaretAssign => "^=",
            AmpAssign => "&=",
            PipeAssign => "|=",
            ShlAssign => "<<=",
            ShrAssign => ">>=",
            ConcatAssign => "..=",
            Unknown => "<unknown>",
        }
    }
}

pub fn keyword(text: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match text {
        "and" => And,
        "break" => Break,
        "do" => Do,
        "else" => Else,
        "elseif" => Elseif,
        "end" => End,
        "false" => False,
        "for" => For,
        "function" => Function,
        "goto" => Goto,
        "if" => If,
        "in" => In,
        "local" => Local,
        "nil" => Nil,
        "not" => Not,
        "or" => Or,
        "repeat" => Repeat,
        "return" => Return,
        "then" => Then,
        "true" => True,
        "until" => Until,
        "while" => While,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentKind {
    Line,
    Long,
    CStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Comment {
    pub kind: CommentKind,
    pub span: Span,
    /// Span of the comment body without its delimiters.
    pub content: Span,
}

pub struct Lexed {
    pub tokens: Vec<Token>,
    pub comments: Vec<Comment>,
    pub errors: Vec<SyntaxError>,
}

pub fn lex(source: &str) -> Lexed {
    let mut lexer = Lexer {
        src: source.as_bytes(),
        pos: 0,
        tokens: Vec::with_capacity(source.len() / 5),
        comments: Vec::new(),
        errors: Vec::new(),
    };
    lexer.run();
    Lexed { tokens: lexer.tokens, comments: lexer.comments, errors: lexer.errors }
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    comments: Vec<Comment>,
    errors: Vec<SyntaxError>,
}

impl Lexer<'_> {
    fn peek(&self, ahead: usize) -> u8 {
        self.src.get(self.pos + ahead).copied().unwrap_or(0)
    }

    fn error(&mut self, start: usize, end: usize, message: impl Into<String>) {
        self.errors.push(SyntaxError { span: Span::new(start as u32, end as u32), message: message.into() });
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token { kind, span: Span::new(start as u32, self.pos as u32) });
    }

    fn run(&mut self) {
        if self.src.starts_with(b"\xEF\xBB\xBF") {
            self.pos = 3;
        }
        if self.peek(0) == b'#' && self.peek(1) == b'!' {
            while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                self.pos += 1;
            }
        }
        loop {
            self.skip_whitespace();
            let start = self.pos;
            if start >= self.src.len() {
                self.push(TokenKind::Eof, start);
                return;
            }
            self.next_token(start);
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.src.len() && matches!(self.src[self.pos], b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C) {
            self.pos += 1;
        }
    }

    fn next_token(&mut self, start: usize) {
        use TokenKind::*;
        let c = self.src[start];
        let kind = match c {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                while matches!(self.peek(0), b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
                    self.pos += 1;
                }
                let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("");
                keyword(text).unwrap_or(Name)
            }
            b'0'..=b'9' => {
                self.number();
                Number
            }
            b'.' if self.peek(1).is_ascii_digit() => {
                self.number();
                Number
            }
            b'"' | b'\'' => {
                self.short_string(c);
                String
            }
            b'`' => {
                self.pos += 1;
                while self.pos < self.src.len() && !matches!(self.src[self.pos], b'`' | b'\n') {
                    self.pos += 1;
                }
                if self.peek(0) == b'`' {
                    self.pos += 1;
                } else {
                    self.error(start, self.pos, "unfinished hash literal");
                }
                JenkinsHash
            }
            b'-' if self.peek(1) == b'-' => {
                self.pos += 2;
                self.comment(start);
                return;
            }
            b'/' if self.peek(1) == b'*' => {
                self.pos += 2;
                let content_start = self.pos;
                let mut content_end = self.src.len();
                let mut closed = false;
                while self.pos < self.src.len() {
                    if self.src[self.pos] == b'*' && self.peek(1) == b'/' {
                        content_end = self.pos;
                        self.pos += 2;
                        closed = true;
                        break;
                    }
                    self.pos += 1;
                }
                if !closed {
                    self.error(start, start + 2, "unfinished comment");
                }
                self.comments.push(Comment {
                    kind: CommentKind::CStyle,
                    span: Span::new(start as u32, self.pos as u32),
                    content: Span::new(content_start as u32, content_end as u32),
                });
                return;
            }
            b'[' => {
                if let Some(level) = self.long_bracket_level() {
                    let (_, closed) = self.long_bracket_body(level);
                    if !closed {
                        self.error(start, start + level + 2, "unfinished long string");
                    }
                    LongString
                } else {
                    self.pos += 1;
                    LBracket
                }
            }
            _ => self.symbol(c),
        };
        self.push(kind, start);
    }

    fn symbol(&mut self, c: u8) -> TokenKind {
        use TokenKind::*;
        let (kind, len) = match (c, self.peek(1), self.peek(2)) {
            (b'.', b'.', b'.') => (Ellipsis, 3),
            (b'.', b'.', b'=') => (ConcatAssign, 3),
            (b'.', b'.', _) => (Concat, 2),
            (b'.', _, _) => (Dot, 1),
            (b'/', b'/', b'=') => (DoubleSlashAssign, 3),
            (b'/', b'/', _) => (DoubleSlash, 2),
            (b'/', b'=', _) => (SlashAssign, 2),
            (b'/', _, _) => (Slash, 1),
            (b'<', b'<', b'=') => (ShlAssign, 3),
            (b'<', b'<', _) => (Shl, 2),
            (b'<', b'=', _) => (Le, 2),
            (b'<', _, _) => (Lt, 1),
            (b'>', b'>', b'=') => (ShrAssign, 3),
            (b'>', b'>', _) => (Shr, 2),
            (b'>', b'=', _) => (Ge, 2),
            (b'>', _, _) => (Gt, 1),
            (b'=', b'=', _) => (Eq, 2),
            (b'=', _, _) => (Assign, 1),
            (b'~', b'=', _) => (Ne, 2),
            (b'!', b'=', _) => (Ne, 2),
            (b'~', _, _) => (Tilde, 1),
            (b':', b':', _) => (DoubleColon, 2),
            (b':', _, _) => (Colon, 1),
            (b'+', b'=', _) => (PlusAssign, 2),
            (b'+', _, _) => (Plus, 1),
            (b'-', b'=', _) => (MinusAssign, 2),
            (b'-', _, _) => (Minus, 1),
            (b'*', b'=', _) => (StarAssign, 2),
            (b'*', _, _) => (Star, 1),
            (b'%', b'=', _) => (PercentAssign, 2),
            (b'%', _, _) => (Percent, 1),
            (b'^', b'=', _) => (CaretAssign, 2),
            (b'^', _, _) => (Caret, 1),
            (b'&', b'=', _) => (AmpAssign, 2),
            (b'&', _, _) => (Amp, 1),
            (b'|', b'=', _) => (PipeAssign, 2),
            (b'|', _, _) => (Pipe, 1),
            (b'#', _, _) => (Pound, 1),
            (b'(', _, _) => (LParen, 1),
            (b')', _, _) => (RParen, 1),
            (b'{', _, _) => (LBrace, 1),
            (b'}', _, _) => (RBrace, 1),
            (b']', _, _) => (RBracket, 1),
            (b';', _, _) => (Semi, 1),
            (b',', _, _) => (Comma, 1),
            (b'?', _, _) => (Question, 1),
            _ => {
                let start = self.pos;
                self.pos += 1;
                while self.pos < self.src.len() && (self.src[self.pos] & 0xC0) == 0x80 {
                    self.pos += 1;
                }
                let end = self.pos;
                let text = std::string::String::from_utf8_lossy(&self.src[start..end]).into_owned();
                self.error(start, end, format!("unexpected symbol '{text}'"));
                return Unknown;
            }
        };
        self.pos += len;
        kind
    }

    fn number(&mut self) {
        let is_hex = self.peek(0) == b'0' && matches!(self.peek(1), b'x' | b'X');
        if is_hex {
            self.pos += 2;
        }
        loop {
            let c = self.peek(0);
            let exponent = if is_hex { matches!(c, b'p' | b'P') } else { matches!(c, b'e' | b'E') };
            if exponent && matches!(self.peek(1), b'+' | b'-') {
                self.pos += 2;
            } else if c.is_ascii_alphanumeric() || c == b'_' {
                self.pos += 1;
            } else if c == b'.' && self.peek(1) != b'.' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn short_string(&mut self, quote: u8) {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.peek(0) {
                0 if self.pos >= self.src.len() => {
                    self.error(start, self.pos, "unfinished string");
                    return;
                }
                b'\n' => {
                    self.error(start, self.pos, "unfinished string");
                    return;
                }
                b'\\' => {
                    self.pos += 1;
                    match self.peek(0) {
                        b'z' => {
                            self.pos += 1;
                            self.skip_whitespace();
                        }
                        b'\r' => {
                            self.pos += 1;
                            if self.peek(0) == b'\n' {
                                self.pos += 1;
                            }
                        }
                        _ if self.pos < self.src.len() => self.pos += 1,
                        _ => {}
                    }
                }
                c if c == quote => {
                    self.pos += 1;
                    return;
                }
                _ => self.pos += 1,
            }
        }
    }

    fn long_bracket_level(&self) -> Option<usize> {
        let mut level = 0;
        while self.peek(1 + level) == b'=' {
            level += 1;
        }
        (self.peek(1 + level) == b'[').then_some(level)
    }

    /// Consumes `[==[ ... ]==]` starting at the opening bracket and returns the content span.
    fn long_bracket_body(&mut self, level: usize) -> (Span, bool) {
        self.pos += level + 2;
        let content_start = self.pos;
        while self.pos < self.src.len() {
            if self.src[self.pos] == b']' {
                let mut i = 0;
                while self.peek(1 + i) == b'=' {
                    i += 1;
                }
                if i == level && self.peek(1 + i) == b']' {
                    let content = Span::new(content_start as u32, self.pos as u32);
                    self.pos += level + 2;
                    return (content, true);
                }
            }
            self.pos += 1;
        }
        (Span::new(content_start as u32, self.pos as u32), false)
    }

    fn comment(&mut self, start: usize) {
        if self.peek(0) == b'[' {
            if let Some(level) = self.long_bracket_level() {
                let (content, closed) = self.long_bracket_body(level);
                if !closed {
                    self.error(start, start + level + 4, "unfinished long comment");
                }
                self.comments.push(Comment {
                    kind: CommentKind::Long,
                    span: Span::new(start as u32, self.pos as u32),
                    content,
                });
                return;
            }
        }
        let content_start = self.pos;
        while self.pos < self.src.len() && !matches!(self.src[self.pos], b'\n' | b'\r') {
            self.pos += 1;
        }
        self.comments.push(Comment {
            kind: CommentKind::Line,
            span: Span::new(start as u32, self.pos as u32),
            content: Span::new(content_start as u32, self.pos as u32),
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumberValue {
    Int(i64),
    Float(f64),
}

pub fn parse_number(text: &str) -> Option<NumberValue> {
    let lower = text.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("0x") {
        if !hex.contains(['.', 'p']) {
            if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let value = hex.bytes().fold(0u64, |acc, b| {
                acc.wrapping_mul(16).wrapping_add((b as char).to_digit(16).unwrap_or(0) as u64)
            });
            return Some(NumberValue::Int(value as i64));
        }
        return parse_hex_float(hex).map(NumberValue::Float);
    }
    if !lower.contains(['.', 'e', 'n', 'i']) {
        if let Ok(value) = lower.parse::<i64>() {
            return Some(NumberValue::Int(value));
        }
    }
    if lower.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'+' | b'-')) {
        return lower.parse::<f64>().ok().map(NumberValue::Float);
    }
    None
}

fn parse_hex_float(hex: &str) -> Option<f64> {
    let (mantissa, exponent) = match hex.split_once('p') {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (hex, 0),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    let mut value = 0f64;
    for b in int_part.bytes() {
        value = value * 16.0 + (b as char).to_digit(16)? as f64;
    }
    let mut scale = 1.0 / 16.0;
    for b in frac_part.bytes() {
        value += (b as char).to_digit(16)? as f64 * scale;
        scale /= 16.0;
    }
    Some(value * 2f64.powi(exponent))
}

pub struct DecodedString {
    pub value: String,
    pub errors: Vec<(Span, &'static str)>,
}

/// Decodes the raw text of a `String` or `LongString` token, delimiters included.
pub fn decode_string(raw: &str, token_start: u32) -> DecodedString {
    let bytes = raw.as_bytes();
    let mut errors = Vec::new();
    if bytes.first() == Some(&b'[') {
        let level = bytes[1..].iter().take_while(|&&b| b == b'=').count();
        let open = level + 2;
        let mut body = raw.get(open..).unwrap_or("");
        let close_len = level + 2;
        if body.len() >= close_len && body.ends_with(']') && raw.len() >= open + close_len {
            body = &body[..body.len() - close_len];
        }
        let body = body.strip_prefix("\r\n").or_else(|| body.strip_prefix('\n')).unwrap_or(body);
        return DecodedString { value: body.to_string(), errors };
    }

    let quote = bytes.first().copied().unwrap_or(b'"');
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == quote {
            break;
        }
        if b != b'\\' {
            out.push(b);
            i += 1;
            continue;
        }
        let escape_start = i;
        i += 1;
        let Some(&e) = bytes.get(i) else { break };
        i += 1;
        match e {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'\\' | b'"' | b'\'' => out.push(e),
            b'\n' => out.push(b'\n'),
            b'\r' => {
                if bytes.get(i) == Some(&b'\n') {
                    i += 1;
                }
                out.push(b'\n');
            }
            b'z' => {
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            }
            b'x' => {
                let digits = raw.get(i..i + 2).filter(|d| d.bytes().all(|b| b.is_ascii_hexdigit()));
                match digits {
                    Some(d) => {
                        out.push(u8::from_str_radix(d, 16).unwrap_or(0));
                        i += 2;
                    }
                    None => errors.push((escape_span(token_start, escape_start, i), "hexadecimal digit expected")),
                }
            }
            b'0'..=b'9' => {
                let mut value = (e - b'0') as u32;
                let mut count = 1;
                while count < 3 && i < bytes.len() && bytes[i].is_ascii_digit() {
                    value = value * 10 + (bytes[i] - b'0') as u32;
                    i += 1;
                    count += 1;
                }
                if value > 255 {
                    errors.push((escape_span(token_start, escape_start, i), "decimal escape too large"));
                }
                out.push(value as u8);
            }
            b'u' => {
                let close = raw[i..].find('}');
                let code = match (bytes.get(i), close) {
                    (Some(b'{'), Some(close)) => u32::from_str_radix(&raw[i + 1..i + close], 16).ok().map(|c| (c, close)),
                    _ => None,
                };
                match code {
                    Some((code, close)) => {
                        i += close + 1;
                        let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    }
                    None => errors.push((escape_span(token_start, escape_start, i), "malformed \\u{XXX} escape")),
                }
            }
            _ => errors.push((escape_span(token_start, escape_start, i), "invalid escape sequence")),
        }
    }
    DecodedString { value: String::from_utf8_lossy(&out).into_owned(), errors }
}

fn escape_span(token_start: u32, start: usize, end: usize) -> Span {
    Span::new(token_start + start as u32, token_start + end as u32)
}
