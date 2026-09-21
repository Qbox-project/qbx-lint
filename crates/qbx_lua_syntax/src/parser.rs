use smol_str::SmolStr;

use crate::ast::*;
use crate::lexer::{self, decode_string, parse_number, NumberValue, Token, TokenKind};
use crate::span::Span;
use crate::SyntaxError;

const MAX_DEPTH: u32 = 180;

pub fn parse(source: &str) -> Chunk {
    let lexed = lexer::lex(source);
    let mut parser = Parser { source, tokens: &lexed.tokens, pos: 0, errors: lexed.errors, depth: 0 };
    let block = parser.parse_chunk();
    let mut errors = parser.errors;
    errors.sort_by_key(|e| e.span.start);
    errors.dedup_by_key(|e| e.span.start);
    Chunk { block, comments: lexed.comments, tokens: lexed.tokens, errors }
}

struct Parser<'a> {
    source: &'a str,
    tokens: &'a [Token],
    pos: usize,
    errors: Vec<SyntaxError>,
    depth: u32,
}

const UNARY_PRIORITY: u8 = 12;

fn binary_op(kind: TokenKind) -> Option<(BinOp, u8, u8)> {
    use TokenKind as T;
    Some(match kind {
        T::Or => (BinOp::Or, 1, 1),
        T::And => (BinOp::And, 2, 2),
        T::Lt => (BinOp::Lt, 3, 3),
        T::Gt => (BinOp::Gt, 3, 3),
        T::Le => (BinOp::Le, 3, 3),
        T::Ge => (BinOp::Ge, 3, 3),
        T::Ne => (BinOp::Ne, 3, 3),
        T::Eq => (BinOp::Eq, 3, 3),
        T::Pipe => (BinOp::BOr, 4, 4),
        T::Tilde => (BinOp::BXor, 5, 5),
        T::Amp => (BinOp::BAnd, 6, 6),
        T::Shl => (BinOp::Shl, 7, 7),
        T::Shr => (BinOp::Shr, 7, 7),
        T::Concat => (BinOp::Concat, 9, 8),
        T::Plus => (BinOp::Add, 10, 10),
        T::Minus => (BinOp::Sub, 10, 10),
        T::Star => (BinOp::Mul, 11, 11),
        T::Slash => (BinOp::Div, 11, 11),
        T::DoubleSlash => (BinOp::IDiv, 11, 11),
        T::Percent => (BinOp::Mod, 11, 11),
        T::Caret => (BinOp::Pow, 14, 13),
        _ => return None,
    })
}

fn compound_op(kind: TokenKind) -> Option<BinOp> {
    use TokenKind as T;
    Some(match kind {
        T::PlusAssign => BinOp::Add,
        T::MinusAssign => BinOp::Sub,
        T::StarAssign => BinOp::Mul,
        T::SlashAssign => BinOp::Div,
        T::DoubleSlashAssign => BinOp::IDiv,
        T::PercentAssign => BinOp::Mod,
        T::CaretAssign => BinOp::Pow,
        T::AmpAssign => BinOp::BAnd,
        T::PipeAssign => BinOp::BOr,
        T::ShlAssign => BinOp::Shl,
        T::ShrAssign => BinOp::Shr,
        T::ConcatAssign => BinOp::Concat,
        _ => return None,
    })
}

impl<'a> Parser<'a> {
    fn tok(&self) -> Token {
        self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn kind(&self) -> TokenKind {
        self.tok().kind
    }

    fn peek_kind(&self, ahead: usize) -> TokenKind {
        self.tokens[(self.pos + ahead).min(self.tokens.len() - 1)].kind
    }

    fn peek_tok(&self, ahead: usize) -> Token {
        self.tokens[(self.pos + ahead).min(self.tokens.len() - 1)]
    }

    fn prev_end(&self) -> u32 {
        if self.pos == 0 {
            0
        } else {
            self.tokens[self.pos - 1].span.end
        }
    }

    fn bump(&mut self) -> Token {
        let tok = self.tok();
        if tok.kind != TokenKind::Eof {
            self.pos += 1;
        }
        tok
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.kind() == kind
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Token> {
        self.at(kind).then(|| self.bump())
    }

    fn text(&self, tok: Token) -> &'a str {
        tok.span.text(self.source)
    }

    fn describe_current(&self) -> String {
        let tok = self.tok();
        match tok.kind {
            TokenKind::Eof => "<eof>".to_string(),
            TokenKind::String | TokenKind::LongString => {
                let text = self.text(tok);
                let short: String = text.chars().take(24).collect();
                if short.len() < text.len() {
                    format!("{short}...")
                } else {
                    short
                }
            }
            _ => self.text(tok).to_string(),
        }
    }

    fn error_at(&mut self, span: Span, message: impl Into<String>) {
        self.errors.push(SyntaxError { span, message: message.into() });
    }

    fn error_here(&mut self, message: impl Into<String>) {
        let span = self.tok().span;
        self.error_at(span, message);
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if let Some(tok) = self.eat(kind) {
            return Some(tok);
        }
        let near = self.describe_current();
        self.error_here(format!("expected '{}' near '{}'", kind.describe(), near));
        None
    }

    fn expect_closing(&mut self, kind: TokenKind, opener: &str, open_span: Span) -> Span {
        if let Some(tok) = self.eat(kind) {
            return tok.span;
        }
        let near = self.describe_current();
        self.error_here(format!("expected '{}' to close '{}' near '{}'", kind.describe(), opener, near));
        if self.at(TokenKind::Eof) {
            self.error_at(open_span, format!("'{}' is never closed", opener));
        }
        Span::empty(self.prev_end())
    }

    fn name_from(&self, tok: Token) -> Name {
        Name { text: SmolStr::new(self.text(tok)), span: tok.span }
    }

    fn missing_name(&self) -> Name {
        Name { text: SmolStr::default(), span: Span::empty(self.prev_end()) }
    }

    fn expect_name(&mut self) -> Name {
        if self.at(TokenKind::Name) {
            let tok = self.bump();
            return self.name_from(tok);
        }
        let near = self.describe_current();
        self.error_here(format!("expected <name> near '{near}'"));
        self.missing_name()
    }

    /// A name after `.` or `:`; keywords are accepted with an error so `foo.end` still produces a field node.
    fn expect_member_name(&mut self, after: Token) -> Name {
        if self.at(TokenKind::Name) {
            let tok = self.bump();
            return self.name_from(tok);
        }
        let tok = self.tok();
        let same_line = !self.source[after.span.end as usize..tok.span.start as usize].contains('\n');
        if tok.kind.is_keyword() && same_line {
            self.error_at(tok.span, format!("'{}' is a keyword and cannot be used as a field name", self.text(tok)));
            self.bump();
            return self.name_from(tok);
        }
        self.error_at(Span::empty(after.span.end), "expected <name> after member access");
        Name { text: SmolStr::default(), span: Span::empty(after.span.end) }
    }

    fn parse_chunk(&mut self) -> Block {
        let start = self.tok().span.start;
        let mut stmts = Vec::new();
        loop {
            self.parse_block_into(&mut stmts);
            if self.at(TokenKind::Eof) {
                break;
            }
            let near = self.describe_current();
            self.error_here(format!("unexpected '{near}'"));
            self.bump();
        }
        Block { stmts, span: Span::new(start, self.prev_end().max(start)) }
    }

    fn block_follows(&self) -> bool {
        matches!(self.kind(), TokenKind::Eof | TokenKind::End | TokenKind::Else | TokenKind::Elseif | TokenKind::Until)
    }

    fn parse_block(&mut self) -> Block {
        let start = self.tok().span.start;
        let mut stmts = Vec::new();
        self.parse_block_into(&mut stmts);
        let end = stmts.last().map_or(start, |s: &Stmt| s.span.end);
        Block { stmts, span: Span::new(start, end.max(start)) }
    }

    fn parse_block_into(&mut self, stmts: &mut Vec<Stmt>) {
        let mut returned: Option<Span> = None;
        while !self.block_follows() {
            let before = self.pos;
            if self.eat(TokenKind::Semi).is_some() {
                continue;
            }
            if let Some(span) = returned.take() {
                self.error_at(span, "'return' must be the last statement in a block");
            }
            let stmt = self.parse_stmt();
            if matches!(stmt.kind, StmtKind::Return(_)) {
                returned = Some(stmt.span);
            }
            stmts.push(stmt);
            if self.pos == before {
                let near = self.describe_current();
                self.error_here(format!("unexpected '{near}'"));
                self.bump();
            }
        }
    }

    fn parse_stmt(&mut self) -> Stmt {
        if self.depth >= MAX_DEPTH {
            return self.too_deep_stmt();
        }
        self.depth += 1;
        let start = self.tok().span;
        let kind = self.parse_stmt_kind();
        self.depth -= 1;
        Stmt { kind, span: Span::new(start.start, self.prev_end().max(start.start)) }
    }

    fn too_deep_stmt(&mut self) -> Stmt {
        let span = self.tok().span;
        self.error_at(span, "chunk has too many nesting levels");
        while !self.at(TokenKind::Eof) {
            self.bump();
        }
        Stmt { kind: StmtKind::Error, span }
    }

    fn parse_stmt_kind(&mut self) -> StmtKind {
        let tok = self.tok();
        match tok.kind {
            TokenKind::If => self.parse_if(),
            TokenKind::While => {
                self.bump();
                let cond = self.parse_expr();
                self.expect(TokenKind::Do);
                let body = self.parse_block();
                self.expect_closing(TokenKind::End, "while", tok.span);
                StmtKind::While { cond, body }
            }
            TokenKind::Do => {
                self.bump();
                let body = self.parse_block();
                self.expect_closing(TokenKind::End, "do", tok.span);
                StmtKind::Do(body)
            }
            TokenKind::For => self.parse_for(),
            TokenKind::Repeat => {
                self.bump();
                let body = self.parse_block();
                if self.eat(TokenKind::Until).is_none() {
                    let near = self.describe_current();
                    self.error_here(format!("expected 'until' to close 'repeat' near '{near}'"));
                }
                let cond = self.parse_expr();
                StmtKind::Repeat { body, cond }
            }
            TokenKind::Function => {
                self.bump();
                let name = self.parse_func_name();
                let func = self.parse_func_body(tok.span);
                StmtKind::Function { name, func: Box::new(func) }
            }
            TokenKind::Local => {
                self.bump();
                if self.eat(TokenKind::Function).is_some() {
                    let name = self.expect_name();
                    let func = self.parse_func_body(tok.span);
                    StmtKind::LocalFunction { name, func: Box::new(func) }
                } else {
                    self.parse_local()
                }
            }
            TokenKind::DoubleColon => {
                self.bump();
                let name = self.expect_name();
                self.expect(TokenKind::DoubleColon);
                StmtKind::Label(name)
            }
            TokenKind::Return => {
                self.bump();
                let mut exprs = Vec::new();
                if !self.block_follows() && !self.at(TokenKind::Semi) {
                    exprs = self.parse_expr_list();
                }
                self.eat(TokenKind::Semi);
                StmtKind::Return(exprs)
            }
            TokenKind::Break => {
                self.bump();
                StmtKind::Break
            }
            TokenKind::Goto => {
                self.bump();
                StmtKind::Goto(self.expect_name())
            }
            TokenKind::Name if self.text(tok) == "defer" && self.defer_follows() => {
                self.bump();
                let body = self.parse_block();
                self.expect_closing(TokenKind::End, "defer", tok.span);
                StmtKind::Defer(body)
            }
            _ => self.parse_expr_stmt(),
        }
    }

    fn defer_follows(&self) -> bool {
        use TokenKind as T;
        !matches!(
            self.peek_kind(1),
            T::Assign
                | T::Dot
                | T::Colon
                | T::Comma
                | T::LParen
                | T::LBracket
                | T::LBrace
                | T::String
                | T::LongString
                | T::Question
                | T::Eof
        ) && compound_op(self.peek_kind(1)).is_none()
    }

    fn parse_if(&mut self) -> StmtKind {
        let if_tok = self.bump();
        let mut branches = Vec::new();
        let mut else_block = None;
        let mut keyword_span = if_tok.span;
        loop {
            let cond = self.parse_expr();
            self.expect(TokenKind::Then);
            let block = self.parse_block();
            branches.push(IfBranch { cond, block, keyword_span });
            match self.kind() {
                TokenKind::Elseif => keyword_span = self.bump().span,
                TokenKind::Else => {
                    self.bump();
                    else_block = Some(self.parse_block());
                    break;
                }
                _ => break,
            }
        }
        self.expect_closing(TokenKind::End, "if", if_tok.span);
        StmtKind::If { branches, else_block }
    }

    fn parse_for(&mut self) -> StmtKind {
        let for_tok = self.bump();
        let first = self.expect_name();
        if self.eat(TokenKind::Assign).is_some() {
            let start = self.parse_expr();
            self.expect(TokenKind::Comma);
            let limit = self.parse_expr();
            let step = self.eat(TokenKind::Comma).map(|_| self.parse_expr());
            self.expect(TokenKind::Do);
            let body = self.parse_block();
            self.expect_closing(TokenKind::End, "for", for_tok.span);
            return StmtKind::NumericFor { var: first, start, limit, step, body };
        }
        let mut names = vec![first];
        while self.eat(TokenKind::Comma).is_some() {
            names.push(self.expect_name());
        }
        let mut exprs = Vec::new();
        if self.expect(TokenKind::In).is_some() {
            exprs = self.parse_expr_list();
        }
        self.expect(TokenKind::Do);
        let body = self.parse_block();
        self.expect_closing(TokenKind::End, "for", for_tok.span);
        StmtKind::GenericFor { names, exprs, body }
    }

    fn parse_func_name(&mut self) -> FuncName {
        let base = self.expect_name();
        let start = base.span;
        let mut path = Vec::new();
        let mut method = None;
        while self.at(TokenKind::Dot) {
            let dot = self.bump();
            path.push(self.expect_member_name(dot));
        }
        if self.at(TokenKind::Colon) {
            let colon = self.bump();
            method = Some(self.expect_member_name(colon));
        }
        FuncName { base, path, method, span: Span::new(start.start, self.prev_end().max(start.start)) }
    }

    fn parse_func_body(&mut self, keyword_span: Span) -> FuncBody {
        let mut params = Vec::new();
        let mut vararg = None;
        let params_start = self.tok().span.start;
        if self.expect(TokenKind::LParen).is_some() {
            loop {
                match self.kind() {
                    TokenKind::Name => {
                        let tok = self.bump();
                        params.push(self.name_from(tok));
                    }
                    TokenKind::Ellipsis => {
                        vararg = Some(self.bump().span);
                        break;
                    }
                    TokenKind::RParen if params.is_empty() => break,
                    _ => {
                        let near = self.describe_current();
                        self.error_here(format!("expected <name> or '...' near '{near}'"));
                        break;
                    }
                }
                if self.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(TokenKind::RParen);
        }
        let params_span = Span::new(params_start, self.prev_end().max(params_start));
        let body = self.parse_block();
        let end_span = self.expect_closing(TokenKind::End, "function", keyword_span);
        FuncBody { params, vararg, params_span, body, span: Span::new(keyword_span.start, self.prev_end()), end_span }
    }

    fn parse_local(&mut self) -> StmtKind {
        let mut names = Vec::new();
        loop {
            let name = self.expect_name();
            let mut attrib = None;
            if self.at(TokenKind::Lt) && self.peek_kind(1) == TokenKind::Name && self.peek_kind(2) == TokenKind::Gt {
                let open = self.bump();
                let attrib_tok = self.bump();
                let close = self.bump();
                let kind = match self.text(attrib_tok) {
                    "const" => Attrib::Const,
                    "close" => Attrib::Close,
                    other => {
                        self.error_at(attrib_tok.span, format!("unknown attribute '{other}'"));
                        Attrib::Unknown
                    }
                };
                attrib = Some((kind, open.span.to(close.span)));
            }
            names.push(AttribName { name, attrib });
            if self.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if self.eat(TokenKind::Assign).is_some() {
            let exprs = self.parse_expr_list();
            return StmtKind::Local { names, exprs, in_unpack: false };
        }
        if self.eat(TokenKind::In).is_some() {
            let exprs = vec![self.parse_expr()];
            return StmtKind::Local { names, exprs, in_unpack: true };
        }
        StmtKind::Local { names, exprs: Vec::new(), in_unpack: false }
    }

    fn parse_expr_stmt(&mut self) -> StmtKind {
        let start = self.pos;
        let expr = self.parse_suffixed_expr();
        if self.pos == start {
            return StmtKind::Error;
        }
        if self.at(TokenKind::Assign) || self.at(TokenKind::Comma) {
            let mut targets = vec![expr];
            while self.eat(TokenKind::Comma).is_some() {
                targets.push(self.parse_suffixed_expr());
            }
            for target in &targets {
                self.check_assignable(target);
            }
            let mut exprs = Vec::new();
            if self.expect(TokenKind::Assign).is_some() {
                exprs = self.parse_expr_list();
            }
            return StmtKind::Assign { targets, exprs };
        }
        if let Some(op) = compound_op(self.kind()) {
            let op_span = self.bump().span;
            self.check_assignable(&expr);
            let value = self.parse_expr();
            return StmtKind::CompoundAssign { target: expr, op, op_span, expr: value };
        }
        if !expr.is_call() && !matches!(expr.kind, ExprKind::Error) {
            let near = self.describe_current();
            self.error_at(expr.span, format!("syntax error near '{near}': expression is not a statement"));
        }
        StmtKind::Expr(expr)
    }

    fn check_assignable(&mut self, target: &Expr) {
        let ok = match &target.kind {
            // CfxLua also accepts `a?.b = v`, which skips the write when `a` is nil.
            ExprKind::Name(_) | ExprKind::Error | ExprKind::Index { .. } | ExprKind::Field { .. } => true,
            _ => false,
        };
        if !ok {
            self.error_at(target.span, "cannot assign to this expression");
        }
    }

    fn parse_expr_list(&mut self) -> Vec<Expr> {
        let mut exprs = vec![self.parse_expr()];
        while self.eat(TokenKind::Comma).is_some() {
            exprs.push(self.parse_expr());
        }
        exprs
    }

    pub(crate) fn parse_expr(&mut self) -> Expr {
        self.parse_sub_expr(0)
    }

    fn parse_sub_expr(&mut self, limit: u8) -> Expr {
        if self.depth >= MAX_DEPTH {
            let span = self.tok().span;
            self.error_at(span, "expression has too many nesting levels");
            while !self.at(TokenKind::Eof) {
                self.bump();
            }
            return Expr { kind: ExprKind::Error, span };
        }
        self.depth += 1;
        let start = self.tok().span.start;
        let unary = match self.kind() {
            TokenKind::Not => Some(UnOp::Not),
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Tilde => Some(UnOp::BNot),
            TokenKind::Pound => Some(UnOp::Len),
            _ => None,
        };
        let mut lhs = if let Some(op) = unary {
            self.bump();
            let operand = self.parse_sub_expr(UNARY_PRIORITY);
            Expr {
                span: Span::new(start, operand.span.end.max(start)),
                kind: ExprKind::Unary { op, expr: Box::new(operand) },
            }
        } else {
            self.parse_simple_expr()
        };
        while let Some((op, left, right)) = binary_op(self.kind()) {
            if left <= limit {
                break;
            }
            let op_span = self.bump().span;
            let rhs = self.parse_sub_expr(right);
            lhs = Expr {
                span: Span::new(start, rhs.span.end.max(op_span.end)),
                kind: ExprKind::Binary { op, op_span, lhs: Box::new(lhs), rhs: Box::new(rhs) },
            };
        }
        self.depth -= 1;
        lhs
    }

    fn parse_simple_expr(&mut self) -> Expr {
        let tok = self.tok();
        let kind = match tok.kind {
            TokenKind::Number => {
                self.bump();
                let value = parse_number(self.text(tok)).unwrap_or_else(|| {
                    self.error_at(tok.span, "malformed number");
                    NumberValue::Int(0)
                });
                ExprKind::Number(value)
            }
            TokenKind::String | TokenKind::LongString => {
                self.bump();
                ExprKind::String(self.string_value(tok))
            }
            TokenKind::JenkinsHash => {
                self.bump();
                let text = self.text(tok);
                let inner = text.strip_prefix('`').unwrap_or(text);
                ExprKind::JenkinsHash(SmolStr::new(inner.strip_suffix('`').unwrap_or(inner)))
            }
            TokenKind::Nil => {
                self.bump();
                ExprKind::Nil
            }
            TokenKind::True => {
                self.bump();
                ExprKind::True
            }
            TokenKind::False => {
                self.bump();
                ExprKind::False
            }
            TokenKind::Ellipsis => {
                self.bump();
                ExprKind::Vararg
            }
            TokenKind::LBrace => return self.parse_table(),
            TokenKind::Function => {
                self.bump();
                let func = self.parse_func_body(tok.span);
                let span = func.span;
                return Expr { kind: ExprKind::Function(Box::new(func)), span };
            }
            _ => return self.parse_suffixed_expr(),
        };
        Expr { kind, span: tok.span }
    }

    fn string_value(&mut self, tok: Token) -> SmolStr {
        let decoded = decode_string(self.text(tok), tok.span.start);
        for (span, message) in decoded.errors {
            self.error_at(span, message);
        }
        SmolStr::new(decoded.value)
    }

    fn parse_primary_expr(&mut self) -> Expr {
        let tok = self.tok();
        match tok.kind {
            TokenKind::Name => {
                self.bump();
                Expr { kind: ExprKind::Name(self.name_from(tok)), span: tok.span }
            }
            TokenKind::LParen => {
                self.bump();
                let inner = self.parse_expr();
                let close = self.expect_closing(TokenKind::RParen, "(", tok.span);
                Expr { kind: ExprKind::Paren(Box::new(inner)), span: tok.span.to(close) }
            }
            _ => {
                let near = self.describe_current();
                let span = Span::empty(self.prev_end());
                self.error_at(
                    if tok.kind == TokenKind::Eof { span } else { tok.span },
                    format!("unexpected symbol near '{near}'"),
                );
                Expr { kind: ExprKind::Error, span }
            }
        }
    }

    fn parse_suffixed_expr(&mut self) -> Expr {
        let mut expr = self.parse_primary_expr();
        if matches!(expr.kind, ExprKind::Error) {
            return expr;
        }
        let start = expr.span.start;
        loop {
            let mut safe = false;
            if self.at(TokenKind::Question) {
                let next = self.peek_tok(1);
                let adjacent = next.span.start == self.tok().span.end;
                if adjacent && matches!(next.kind, TokenKind::Dot | TokenKind::LBracket | TokenKind::Colon) {
                    self.bump();
                    safe = true;
                } else {
                    break;
                }
            }
            let tok = self.tok();
            let kind = match tok.kind {
                TokenKind::Dot => {
                    self.bump();
                    let name = self.expect_member_name(tok);
                    ExprKind::Field { base: Box::new(expr), name, safe }
                }
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.parse_expr();
                    self.expect_closing(TokenKind::RBracket, "[", tok.span);
                    ExprKind::Index { base: Box::new(expr), index: Box::new(index), safe }
                }
                TokenKind::Colon => {
                    self.bump();
                    let method = self.expect_member_name(tok);
                    match self.parse_call_args() {
                        Some((args, args_span, style)) => {
                            ExprKind::MethodCall { base: Box::new(expr), method, args, args_span, style, safe }
                        }
                        None => {
                            if !method.is_missing() {
                                self.error_at(
                                    Span::empty(method.span.end),
                                    "expected function arguments after method name",
                                );
                            }
                            let args_span = Span::empty(self.prev_end());
                            ExprKind::MethodCall {
                                base: Box::new(expr),
                                method,
                                args: Vec::new(),
                                args_span,
                                style: CallStyle::Paren,
                                safe,
                            }
                        }
                    }
                }
                TokenKind::LParen | TokenKind::String | TokenKind::LongString | TokenKind::LBrace => {
                    let Some((args, args_span, style)) = self.parse_call_args() else { break };
                    ExprKind::Call { callee: Box::new(expr), args, args_span, style }
                }
                _ => break,
            };
            expr = Expr { kind, span: Span::new(start, self.prev_end().max(start)) };
        }
        expr
    }

    fn parse_call_args(&mut self) -> Option<(Vec<Expr>, Span, CallStyle)> {
        let tok = self.tok();
        match tok.kind {
            TokenKind::LParen => {
                self.bump();
                let mut args = Vec::new();
                if !self.at(TokenKind::RParen) {
                    args = self.parse_expr_list();
                }
                let close = self.expect_closing(TokenKind::RParen, "(", tok.span);
                Some((args, tok.span.to(close), CallStyle::Paren))
            }
            TokenKind::String | TokenKind::LongString => {
                self.bump();
                let value = self.string_value(tok);
                Some((vec![Expr { kind: ExprKind::String(value), span: tok.span }], tok.span, CallStyle::String))
            }
            TokenKind::LBrace => {
                let table = self.parse_table();
                let span = table.span;
                Some((vec![table], span, CallStyle::Table))
            }
            _ => None,
        }
    }

    fn parse_table(&mut self) -> Expr {
        let open = self.bump();
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let before = self.pos;
            let tok = self.tok();
            match tok.kind {
                TokenKind::LBracket => {
                    self.bump();
                    let key = self.parse_expr();
                    self.expect_closing(TokenKind::RBracket, "[", tok.span);
                    self.expect(TokenKind::Assign);
                    let value = self.parse_expr();
                    fields.push(TableField::Keyed { key, value });
                }
                TokenKind::Name if self.peek_kind(1) == TokenKind::Assign => {
                    self.bump();
                    self.bump();
                    let name = self.name_from(tok);
                    let value = self.parse_expr();
                    fields.push(TableField::Named { name, value });
                }
                TokenKind::Dot if self.peek_kind(1) == TokenKind::Name => {
                    self.bump();
                    let name_tok = self.bump();
                    fields.push(TableField::SetMember(self.name_from(name_tok)));
                }
                _ => fields.push(TableField::Positional(self.parse_expr())),
            }
            if self.eat(TokenKind::Comma).is_none() && self.eat(TokenKind::Semi).is_none() {
                break;
            }
            if self.pos == before {
                break;
            }
        }
        let close = self.expect_closing(TokenKind::RBrace, "{", open.span);
        Expr { kind: ExprKind::Table(fields), span: open.span.to(close) }
    }
}
