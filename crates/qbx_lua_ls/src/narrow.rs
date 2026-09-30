//! Type guards: what a condition tells about the locals it tests, in the code that only runs when
//! it held or failed. `if not name then return end` leaves `name` holding a value for the rest of
//! its block, and `if name then ... end` for the branch.

use qbx_lua_analysis::scope::{LocalId, Resolution, Resolved};
use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::Span;
use rustc_hash::FxHashMap;

use crate::infer::always_exits;
use crate::types::Type;

/// What a guard tells about the value of a local.
#[derive(Clone, Debug, PartialEq)]
pub enum Fact {
    /// It is neither `nil` nor `false`.
    Truthy,
    /// It is `nil` or `false`.
    Falsy,
    /// It equals this `nil`, `true` or `false`.
    Is(Type),
    /// It differs from this `nil`, `true` or `false`.
    IsNot(Type),
}

impl Fact {
    /// What is left of `ty` for a value the fact holds for, or `None` when no value of `ty`
    /// satisfies it. A type that says nothing about its values stays as it is.
    pub fn apply(&self, ty: &Type) -> Option<Type> {
        match ty {
            Type::Unknown | Type::Any => Some(match self {
                Fact::Is(value) => value.clone(),
                _ => ty.clone(),
            }),
            Type::Union(parts) => {
                let kept: Vec<Type> = parts.iter().filter_map(|part| self.apply(part)).collect();
                (!kept.is_empty()).then(|| Type::union(kept))
            }
            Type::Boolean => match self {
                Fact::Truthy => Some(Type::BooleanLit(true)),
                Fact::Falsy => Some(Type::BooleanLit(false)),
                Fact::Is(Type::BooleanLit(value)) => Some(Type::BooleanLit(*value)),
                Fact::IsNot(Type::BooleanLit(value)) => Some(Type::BooleanLit(!*value)),
                Fact::Is(_) => None,
                Fact::IsNot(_) => Some(Type::Boolean),
            },
            _ => {
                let falsy = matches!(ty, Type::Nil | Type::BooleanLit(false));
                let holds = match self {
                    Fact::Truthy => !falsy,
                    Fact::Falsy => falsy,
                    Fact::Is(value) => ty == value,
                    Fact::IsNot(value) => ty != value,
                };
                holds.then(|| ty.clone())
            }
        }
    }
}

type Facts = Vec<(LocalId, Fact)>;

/// The facts the guards of a file establish, each for a local and the code it holds in.
#[derive(Debug, Default)]
pub struct Guards {
    facts: FxHashMap<LocalId, Vec<(Span, Fact)>>,
}

impl Guards {
    pub fn of(chunk: &Chunk, resolution: &Resolution) -> Self {
        let mut finder = Finder { resolution, guards: Guards::default() };
        finder.visit_block(&chunk.block);
        finder.guards
    }

    /// What the guards around `offset` tell about `local`.
    pub fn at(&self, local: LocalId, offset: u32) -> impl Iterator<Item = &Fact> {
        let facts = self.facts.get(&local).map(Vec::as_slice).unwrap_or_default();
        facts.iter().filter(move |(span, _)| span.start <= offset && offset < span.end).map(|(_, fact)| fact)
    }
}

struct Finder<'r> {
    resolution: &'r Resolution,
    guards: Guards,
}

impl Finder<'_> {
    /// The local `expr` names, unless something assigns to it after its declaration: a guard says
    /// nothing about a value that may have been replaced since.
    fn local(&self, expr: &Expr) -> Option<LocalId> {
        let ExprKind::Name(name) = &expr.unparen().kind else { return None };
        let Some(Resolved::Local(id)) = self.resolution.resolve_at(name.span.start) else { return None };
        (!self.resolution.local(id).refs.iter().any(|r| r.write)).then_some(id)
    }

    /// Adds what `cond` being true, or false without `holds`, tells about the locals in it.
    fn facts(&self, cond: &Expr, holds: bool, out: &mut Facts) {
        let cond = cond.unparen();
        match &cond.kind {
            ExprKind::Name(_) => {
                if let Some(local) = self.local(cond) {
                    out.push((local, if holds { Fact::Truthy } else { Fact::Falsy }));
                }
            }
            ExprKind::Unary { op: UnOp::Not, expr } => self.facts(expr, !holds, out),
            // Both sides of a true `and` are true, and both sides of a false `or` are false.
            ExprKind::Binary { op: BinOp::And, lhs, rhs, .. } if holds => {
                self.facts(lhs, true, out);
                self.facts(rhs, true, out);
            }
            ExprKind::Binary { op: BinOp::Or, lhs, rhs, .. } if !holds => {
                self.facts(lhs, false, out);
                self.facts(rhs, false, out);
            }
            ExprKind::Binary { op: op @ (BinOp::Eq | BinOp::Ne), lhs, rhs, .. } => {
                let compared = match (self.local(lhs), literal(rhs)) {
                    (Some(local), Some(value)) => Some((local, value)),
                    _ => self.local(rhs).zip(literal(lhs)),
                };
                if let Some((local, value)) = compared {
                    let equal = (*op == BinOp::Eq) == holds;
                    out.push((local, if equal { Fact::Is(value) } else { Fact::IsNot(value) }));
                }
            }
            _ => {}
        }
    }

    fn record(&mut self, span: Span, facts: Facts) {
        for (local, fact) in facts {
            self.guards.facts.entry(local).or_default().push((span, fact));
        }
    }

    /// What holds once `stmt` has run: the conditions of an `if` whose other paths all leave, as
    /// after `if not name then return end`, and the condition of an `assert`.
    fn after(&self, stmt: &Stmt) -> Facts {
        match &stmt.kind {
            StmtKind::If { branches, else_block } => {
                // The ways through the statement that reach the code after it.
                let mut open: Vec<Facts> = Vec::new();
                let mut failed = Facts::new();
                for branch in branches {
                    if !leaves(&branch.block) {
                        let mut facts = failed.clone();
                        self.facts(&branch.cond, true, &mut facts);
                        open.push(facts);
                    }
                    self.facts(&branch.cond, false, &mut failed);
                }
                if !else_block.as_ref().is_some_and(leaves) {
                    open.push(failed);
                }
                match open.len() {
                    1 => open.pop().unwrap_or_default(),
                    _ => Facts::new(),
                }
            }
            StmtKind::Expr(Expr { kind: ExprKind::Call { callee, args, .. }, .. }) => {
                let is_assert = matches!(&callee.kind, ExprKind::Name(name) if name.text == "assert"
                    && !matches!(self.resolution.resolve_at(name.span.start), Some(Resolved::Local(_))));
                let mut facts = Facts::new();
                if let Some(cond) = args.first().filter(|_| is_assert) {
                    self.facts(cond, true, &mut facts);
                }
                facts
            }
            _ => Facts::new(),
        }
    }
}

impl<'ast> Visitor<'ast> for Finder<'_> {
    fn visit_block(&mut self, block: &'ast Block) {
        for (index, stmt) in block.stmts.iter().enumerate() {
            self.visit_stmt(stmt);
            let facts = self.after(stmt);
            if !facts.is_empty() {
                // A label can be reached by a `goto` that skipped the guard.
                let label = block.stmts[index + 1..].iter().find(|next| matches!(next.kind, StmtKind::Label(_)));
                let end = label.map_or(block.span.end, |label| label.span.start);
                self.record(Span { start: stmt.span.end, end: end.max(stmt.span.end) }, facts);
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match &stmt.kind {
            StmtKind::If { branches, else_block } => {
                let else_start = else_block.as_ref().map(|block| block.span.start.min(stmt.span.end));
                let mut failed = Facts::new();
                for (index, branch) in branches.iter().enumerate() {
                    let mut facts = failed.clone();
                    self.facts(&branch.cond, true, &mut facts);
                    // Measured between the keywords so half-typed code inside the branch still counts.
                    let next = branches.get(index + 1).map(|b| b.keyword_span.start);
                    let end = next.or(else_start).unwrap_or(stmt.span.end);
                    self.record(Span { start: branch.cond.span.end, end: end.max(branch.cond.span.end) }, facts);
                    self.facts(&branch.cond, false, &mut failed);
                }
                if let Some(start) = else_start {
                    self.record(Span { start, end: stmt.span.end }, failed);
                }
            }
            StmtKind::While { cond, .. } => {
                let mut facts = Facts::new();
                self.facts(cond, true, &mut facts);
                self.record(Span { start: cond.span.end, end: stmt.span.end.max(cond.span.end) }, facts);
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        // `b` of `a and b` only runs when `a` is true, and `b` of `a or b` when it is false.
        if let ExprKind::Binary { op: op @ (BinOp::And | BinOp::Or), lhs, rhs, .. } = &expr.kind {
            let mut facts = Facts::new();
            self.facts(lhs, *op == BinOp::And, &mut facts);
            self.record(rhs.span, facts);
        }
        visit::walk_expr(self, expr);
    }
}

fn literal(expr: &Expr) -> Option<Type> {
    match expr.unparen().kind {
        ExprKind::Nil => Some(Type::Nil),
        ExprKind::True => Some(Type::BooleanLit(true)),
        ExprKind::False => Some(Type::BooleanLit(false)),
        _ => None,
    }
}

/// Whether running `block` always leaves the block around it: by returning, raising an error,
/// `break` or `goto`.
fn leaves(block: &Block) -> bool {
    match block.stmts.last().map(|stmt| &stmt.kind) {
        Some(StmtKind::Break) => true,
        Some(StmtKind::Do(body)) => leaves(body),
        Some(StmtKind::If { branches, else_block: Some(else_block) }) => {
            branches.iter().all(|branch| leaves(&branch.block)) && leaves(else_block)
        }
        _ => always_exits(block),
    }
}
