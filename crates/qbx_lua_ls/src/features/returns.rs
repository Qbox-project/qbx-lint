//! `return-type-mismatch` and `missing-return`: a function documented with `@return` has to return
//! values of those types. A value of a different kind, or a literal the type does not list, is a
//! mismatch; a `return` with fewer values than the required ones, or a body that can run past its
//! end, is missing one. A value is required unless its type allows `nil`. An empty body is missing
//! its values too, except in a `---@meta` file, whose functions only declare their signatures.
//! Returned tables typed as a class are checked like other class tables, by `missing-fields`,
//! `assign-type-mismatch` and `undeclared-field`.

use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::Span;

use super::class_tables::Classes;
use crate::infer::{always_exits, documented_functions, return_stmts, Infer};
use crate::types::Type;

/// Each returned value that its function's `@return` does not take, with the message naming both
/// types.
pub fn mismatched_returns(infer: &Infer, chunk: &Chunk) -> Vec<(Span, String)> {
    let classes = Classes::new(infer);
    let mut out = Vec::new();
    for (func, returns) in documented(infer, chunk) {
        for (_, exprs) in return_stmts(&func.body) {
            for (i, (given, span)) in returned_values(infer, &classes, exprs).into_iter().enumerate() {
                let Some(expected) = expected_at(&returns, i) else { break };
                if classes.rejects(expected, classes.file(), &given) {
                    let shown =
                        if classes.literal_mismatch(expected, classes.file(), &given) { given } else { given.widen() };
                    let message = format!("Cannot return `{shown}` as return value #{} of type `{expected}`", i + 1);
                    out.push((span, message));
                }
            }
        }
    }
    out
}

/// Each `return` that gives fewer values than its function's `@return` requires, and the `end` of
/// each such function that its body can run past.
pub fn missing_returns(infer: &Infer, chunk: &Chunk) -> Vec<(Span, String)> {
    let classes = Classes::new(infer);
    let is_meta = is_meta_file(infer.ctx.source, chunk);
    let mut out = Vec::new();
    for (func, returns) in documented(infer, chunk) {
        let is_required = |ty: &Type| !matches!(ty, Type::Variadic(_)) && !classes.admits_nil(ty, classes.file());
        let required = returns.iter().rposition(is_required).map_or(0, |last| last + 1);
        if required == 0 || (is_meta && func.body.stmts.is_empty()) {
            continue;
        }
        for (stmt, exprs) in return_stmts(&func.body) {
            // A call or `...` at the end passes as many values as it gives.
            if exprs.len() < required && !exprs.last().is_some_and(Expr::is_multi_value) {
                let message =
                    format!("`@return` requires {}, but this returns {}", values(required), values(exprs.len()));
                out.push((stmt.span, message));
            }
        }
        if !always_exits(&func.body) {
            let types: Vec<String> = returns[..required].iter().map(|ty| format!("`{ty}`")).collect();
            let message = format!(
                "The function can reach its end without returning, but `@return` requires {}",
                types.join(", ")
            );
            out.push((func.end_span, message));
        }
    }
    out
}

/// Whether a `---@meta` line marks the file as a definition file.
fn is_meta_file(source: &str, chunk: &Chunk) -> bool {
    chunk.comments.iter().any(|comment| {
        let doc = comment.span.text(source).strip_prefix("---").unwrap_or_default();
        doc.trim_start()
            .strip_prefix("@meta")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    })
}

fn values(count: usize) -> String {
    match count {
        0 => "no values".to_string(),
        1 => "1 value".to_string(),
        count => format!("{count} values"),
    }
}

/// The `@return` type of the value at `index`; a trailing `...T` covers every value from there on.
fn expected_at(returns: &[Type], index: usize) -> Option<&Type> {
    match returns.get(index) {
        Some(Type::Variadic(inner)) => Some(inner),
        Some(ty) => Some(ty),
        None => match returns.last() {
            Some(Type::Variadic(inner)) => Some(inner),
            _ => None,
        },
    }
}

/// The values one `return` passes with where each is written, the last spread when it is a call.
fn returned_values(infer: &Infer, classes: &Classes, exprs: &[Expr]) -> Vec<(Type, Span)> {
    let mut values = Vec::new();
    for (i, expr) in exprs.iter().enumerate() {
        if i + 1 == exprs.len() && expr.is_call() {
            values.extend(infer.expr_multi(expr).into_iter().map(|ty| (ty, expr.span)));
        } else {
            values.push((classes.value_type(expr), expr.span));
        }
    }
    values
}

/// The functions of `chunk` documented with `@return`, with the types it lists.
fn documented<'c>(infer: &Infer, chunk: &'c Chunk) -> Vec<(&'c FuncBody, Vec<Type>)> {
    let mut finder = Documented { infer, out: Vec::new() };
    finder.visit_block(&chunk.block);
    finder.out
}

struct Documented<'a, 'b, 'c> {
    infer: &'a Infer<'b>,
    out: Vec<(&'c FuncBody, Vec<Type>)>,
}

impl<'c> Visitor<'c> for Documented<'_, '_, 'c> {
    fn visit_stmt(&mut self, stmt: &'c Stmt) {
        let functions = documented_functions(stmt);
        if !functions.is_empty() {
            let doc = self.infer.ctx.doc_at(stmt.span.start);
            if !doc.returns.is_empty() {
                let returns: Vec<Type> = doc.returns.iter().map(|r| r.ty.clone()).collect();
                self.out.extend(functions.into_iter().map(|func| (func, returns.clone())));
            }
        }
        visit::walk_stmt(self, stmt);
    }
}
