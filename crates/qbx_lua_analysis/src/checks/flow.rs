use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::{NumberValue, Span};
use rustc_hash::FxHashMap;

use super::{FileInput, Sink};
use crate::diagnostic::Tag;
use crate::rules;

pub(super) fn check(input: &FileInput, sink: &mut Sink) {
    for label in input.resolution.labels.iter().filter(|l| !l.used) {
        sink.report_with(
            rules::UNUSED_LABEL,
            label.span,
            format!("label '{}' is never used", label.name),
            Some(Tag::Unnecessary),
            None,
        );
    }
    for name in &input.resolution.undefined_gotos {
        sink.report(rules::UNDEFINED_LABEL, name.span, format!("no visible label '{}' for goto", name.text));
    }
    Flow { input, sink }.visit_block(&input.chunk.block);
}

struct Flow<'a, 'b> {
    input: &'a FileInput<'a>,
    sink: &'a mut Sink<'b>,
}

impl Flow<'_, '_> {
    fn has_comment_inside(&self, span: Span) -> bool {
        let comments = &self.input.chunk.comments;
        let first = comments.partition_point(|c| c.span.start < span.start);
        comments.get(first).is_some_and(|c| c.span.end <= span.end)
    }

    fn empty_block(&mut self, block: &Block, region: Span, what: &str) {
        if block.stmts.is_empty() && !self.has_comment_inside(region) {
            self.sink.report(rules::EMPTY_BLOCK, region, format!("empty {what} block"));
        }
    }

    fn unreachable(&mut self, block: &Block) {
        let exit = block.stmts.iter().position(|s| matches!(s.kind, StmtKind::Break | StmtKind::Goto(_)));
        let Some(exit) = exit else { return };
        let Some(next) = block.stmts.get(exit + 1) else { return };
        if matches!(next.kind, StmtKind::Label(_)) {
            return;
        }
        let end = block.stmts.last().map_or(next.span.end, |s| s.span.end);
        self.sink.report_with(
            rules::UNREACHABLE_CODE,
            Span::new(next.span.start, end),
            "unreachable code",
            Some(Tag::Unnecessary),
            None,
        );
    }

    fn balance(&mut self, targets: usize, exprs: &[Expr], stmt_span: Span, is_local: bool) {
        let Some(last) = exprs.last() else { return };
        if exprs.len() > targets {
            let extra = exprs[targets].span.to(last.span);
            self.sink.report(
                rules::UNBALANCED_ASSIGNMENTS,
                extra,
                format!("{} value(s) assigned to {targets} target(s); the extra values are discarded", exprs.len()),
            );
        } else if exprs.len() < targets && !last.is_multi_value() && !is_local {
            self.sink.report(
                rules::UNBALANCED_ASSIGNMENTS,
                stmt_span,
                format!("{} value(s) assigned to {targets} target(s); the remaining targets become nil", exprs.len()),
            );
        }
    }

    fn duplicate_keys(&mut self, fields: &[TableField]) {
        let mut seen: FxHashMap<String, ()> = FxHashMap::default();
        for field in fields {
            let (key, span) = match field {
                TableField::Named { name, .. } => (format!("s:{}", name.text), name.span),
                TableField::SetMember(name) => (format!("s:{}", name.text), name.span),
                TableField::Keyed { key, .. } => match &key.kind {
                    ExprKind::String(s) => (format!("s:{s}"), key.span),
                    ExprKind::Number(NumberValue::Int(i)) => (format!("n:{i}"), key.span),
                    ExprKind::JenkinsHash(h) => (format!("h:{}", h.to_ascii_lowercase()), key.span),
                    _ => continue,
                },
                TableField::Positional(_) => continue,
            };
            if seen.insert(key.clone(), ()).is_some() {
                self.sink.report(rules::DUPLICATE_INDEX, span, format!("duplicate table key '{}'", &key[2..]));
            }
        }
    }
}

fn same_place(a: &Expr, b: &Expr) -> bool {
    match (a.unparen().dotted_path(), b.unparen().dotted_path()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

impl<'ast> Visitor<'ast> for Flow<'_, '_> {
    fn visit_block(&mut self, block: &'ast Block) {
        self.unreachable(block);
        visit::walk_block(self, block);
    }

    fn visit_func_body(&mut self, func: &'ast FuncBody) {
        for (i, param) in func.params.iter().enumerate() {
            if param.text != "_" && func.params[..i].iter().any(|p| p.text == param.text) {
                self.sink.report(rules::DUPLICATE_ARGUMENT, param.span, format!("duplicate argument '{}'", param.text));
            }
        }
        visit::walk_func_body(self, func);
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match &stmt.kind {
            StmtKind::Local { names, exprs, in_unpack: false } => self.balance(names.len(), exprs, stmt.span, true),
            StmtKind::Assign { targets, exprs } => {
                self.balance(targets.len(), exprs, stmt.span, false);
                if let ([target], [value]) = (targets.as_slice(), exprs.as_slice()) {
                    if same_place(target, value) {
                        self.sink.report(rules::SELF_ASSIGNMENT, stmt.span, "value is assigned to itself");
                    }
                }
            }
            StmtKind::Do(body) => self.empty_block(body, stmt.span, "do"),
            StmtKind::While { body, .. } => self.empty_block(body, stmt.span, "while"),
            StmtKind::Repeat { body, .. } => self.empty_block(body, stmt.span, "repeat"),
            StmtKind::NumericFor { body, .. } | StmtKind::GenericFor { body, .. } => {
                self.empty_block(body, stmt.span, "for")
            }
            StmtKind::If { branches, else_block } => {
                for (i, branch) in branches.iter().enumerate() {
                    let end = branches
                        .get(i + 1)
                        .map(|b| b.keyword_span.start)
                        .or(else_block.as_ref().map(|b| b.span.start.max(branch.block.span.end)))
                        .unwrap_or(stmt.span.end);
                    let region = Span::new(branch.keyword_span.start, end.max(branch.keyword_span.end));
                    let what = if i == 0 { "if" } else { "elseif" };
                    self.empty_block(&branch.block, region, what);
                }
                if let Some(block) = else_block {
                    let start = branches.last().map_or(stmt.span.start, |b| b.block.span.end.max(b.cond.span.end));
                    self.empty_block(block, Span::new(start, stmt.span.end), "else");
                }
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        match &expr.kind {
            ExprKind::Table(fields) => self.duplicate_keys(fields),
            ExprKind::Binary { op: BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge, lhs, rhs, .. }
                if same_place(lhs, rhs) =>
            {
                self.sink.report(
                    rules::SELF_COMPARISON,
                    expr.span,
                    "both sides of the comparison are the same expression",
                );
            }
            _ => {}
        }
        visit::walk_expr(self, expr);
    }
}
