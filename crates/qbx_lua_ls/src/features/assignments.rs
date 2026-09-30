//! `assign-type-mismatch` for variables: a `local` or an assignment with a `---@type` above it has
//! to store values of that type, and so does a later assignment to a local declared with
//! `---@type` or to a parameter documented with `@param`. As for `@return`, a value of a different
//! kind, or a literal the type does not list, is a mismatch. `class_tables` checks the fields of
//! class tables.

use qbx_lua_analysis::scope::{LocalId, Resolved};
use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::Span;

use super::class_tables::{mismatched_fields, Classes};
use super::unknown_types::has_param_line;
use crate::infer::{Decl, Infer};
use crate::types::Type;

/// Each value stored in a class field or a variable whose type does not take it, with the message
/// naming both types. A value that its field rejects is reported for the field alone.
pub fn mismatched_assignments(infer: &Infer, chunk: &Chunk) -> Vec<(Span, String)> {
    let mut out = mismatched_fields(infer, chunk);
    let mut finder = Finder { infer, classes: Classes::new(infer), out: Vec::new() };
    finder.visit_block(&chunk.block);
    finder.out.retain(|(span, _)| !out.iter().any(|(reported, _)| reported == span));
    out.append(&mut finder.out);
    out
}

struct Finder<'a, 'b> {
    infer: &'a Infer<'b>,
    classes: Classes<'a, 'b>,
    out: Vec<(Span, String)>,
}

impl Finder<'_, '_> {
    /// The `---@type` above `stmt` for the name at `index`. `---@class Name` above a table
    /// declares the class rather than a value of it.
    fn stmt_type(&self, stmt: &Stmt, index: usize) -> Option<Type> {
        let doc = self.infer.ctx.doc_at(stmt.span.start);
        doc.type_at(index).filter(|_| doc.classes.is_empty()).cloned()
    }

    /// The type a local is declared with: the `---@type` above its `local` statement, or its
    /// `@param` line. What the guards around an assignment tell about the local does not limit
    /// what it may be given.
    fn local_type(&self, id: LocalId) -> Option<Type> {
        let ctx = self.infer.ctx;
        let local = ctx.resolution.local(id);
        match ctx.decl(local.decl.start)? {
            Decl::Local { stmt, index } => self.stmt_type(stmt, *index),
            Decl::Param { .. } if has_param_line(self.infer, local) => Some(self.infer.local_type(id)),
            _ => None,
        }
    }

    /// Reports each of `exprs` that the declared type of the target it is stored in does not take.
    /// A target is where its name is written, with its type when one is declared.
    fn check(&mut self, targets: &[(Span, Option<Type>)], exprs: &[Expr]) {
        if targets.iter().all(|(_, ty)| ty.is_none()) {
            return;
        }
        let from = self.classes.file();
        for ((name, expected), (given, span)) in targets.iter().zip(self.classes.values(exprs)) {
            let Some(expected) = expected else { continue };
            if self.classes.rejects(expected, from, &given) {
                let shown = if self.classes.literal_mismatch(expected, from, &given) { given } else { given.widen() };
                let name = name.text(self.infer.ctx.source);
                self.out.push((span, format!("Cannot assign `{shown}` to `{name}` of type `{expected}`")));
            }
        }
    }
}

impl<'c> Visitor<'c> for Finder<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'c Stmt) {
        match &stmt.kind {
            StmtKind::Local { names, exprs, in_unpack: false } => {
                let declared = |(index, name): (usize, &AttribName)| (name.name.span, self.stmt_type(stmt, index));
                let targets: Vec<_> = names.iter().enumerate().map(declared).collect();
                self.check(&targets, exprs);
            }
            StmtKind::Assign { targets, exprs } => {
                // The `---@type` above the assignment, or else the type its target is declared with.
                let declared = |(index, target): (usize, &Expr)| {
                    let ty = self.stmt_type(stmt, index).or_else(|| match &target.kind {
                        ExprKind::Name(name) => match self.infer.ctx.resolution.resolve_at(name.span.start) {
                            Some(Resolved::Local(id)) => self.local_type(id),
                            _ => None,
                        },
                        _ => None,
                    });
                    (target.span, ty)
                };
                let targets: Vec<_> = targets.iter().enumerate().map(declared).collect();
                self.check(&targets, exprs);
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }
}
