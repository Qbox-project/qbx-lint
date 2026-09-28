use std::sync::Arc;

use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::{SmolStr, Span};
use qbx_luacats::types::FunType;
use rustc_hash::FxHashMap;

use super::crossfile::{passed_count, plural};
use super::{FileInput, Sink};
use crate::project::ResourceEnv;
use crate::rules;
use crate::scope::{LocalId, LocalKind, Resolved};
use crate::side_guard::SideRegions;
use crate::signature::{documented, global_key, member_path, Requirement};

/// Every value assigned to a local or to a field of a local table, by local and dotted field path.
type LocalDefs = FxHashMap<(LocalId, SmolStr), Vec<Option<Arc<FunType>>>>;

pub(super) fn check(input: &FileInput, sink: &mut Sink) {
    if sink.enabled(rules::MISSING_PARAMETER).is_none() {
        return;
    }
    let own_env;
    let env = match input.resource {
        Some(resource) => resource.env,
        None => {
            let mut env = ResourceEnv::default();
            env.add_summary(input.summary, None);
            own_env = env;
            &own_env
        }
    };
    let mut collector = LocalCollector { input, defs: LocalDefs::default() };
    collector.visit_block(&input.chunk.block);
    let mut calls =
        Calls { input, env, locals: collector.defs, regions: SideRegions::of(input.source, input.chunk), sink };
    calls.visit_block(&input.chunk.block);
}

struct LocalCollector<'i, 'a> {
    input: &'i FileInput<'a>,
    defs: LocalDefs,
}

impl LocalCollector<'_, '_> {
    fn local(&self, name: &Name) -> Option<LocalId> {
        match self.input.resolution.resolve_at(name.span.start)? {
            Resolved::Local(id) => Some(id),
            Resolved::Global(_) => None,
        }
    }

    fn add(&mut self, id: LocalId, fields: &[&str], signature: Option<Arc<FunType>>) {
        self.defs.entry((id, SmolStr::new(fields.join(".")))).or_default().push(signature);
    }

    fn signature(&self, stmt: &Stmt, value: Option<&Expr>, single: bool) -> Option<Arc<FunType>> {
        match value.map(|e| &e.kind) {
            // A doc comment above `local a, b = ...` does not say which value it describes.
            Some(ExprKind::Function(func)) if single => {
                documented(self.input.source, &self.input.chunk.comments, stmt.span.start, func, false)
            }
            _ => None,
        }
    }
}

impl<'ast> Visitor<'ast> for LocalCollector<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match &stmt.kind {
            StmtKind::LocalFunction { name, func } => {
                if let Some(id) = self.local(name) {
                    let signature =
                        documented(self.input.source, &self.input.chunk.comments, stmt.span.start, func, false);
                    self.add(id, &[], signature);
                }
            }
            StmtKind::Local { names, exprs, .. } => {
                for (index, name) in names.iter().enumerate() {
                    let Some(id) = self.local(&name.name) else { continue };
                    // `local f` holds nil until it is assigned, which the assignment records.
                    if index >= exprs.len() && !exprs.last().is_some_and(Expr::is_multi_value) {
                        continue;
                    }
                    let signature = self.signature(stmt, exprs.get(index), names.len() == 1);
                    self.add(id, &[], signature);
                }
            }
            StmtKind::Function { name, func } => {
                if let Some(id) = self.local(&name.base) {
                    let fields: Vec<&str> = name.path.iter().chain(&name.method).map(|n| n.text.as_str()).collect();
                    let signature = documented(
                        self.input.source,
                        &self.input.chunk.comments,
                        stmt.span.start,
                        func,
                        name.method.is_some(),
                    );
                    self.add(id, &fields, signature);
                }
            }
            StmtKind::Assign { targets, exprs } => {
                for (index, target) in targets.iter().enumerate() {
                    let Some((root, fields)) = member_path(target) else { continue };
                    if let Some(id) = self.local(root) {
                        let signature = self.signature(stmt, exprs.get(index), targets.len() == 1);
                        self.add(id, &fields, signature);
                    }
                }
            }
            StmtKind::CompoundAssign { target, .. } => {
                if let Some((root, fields)) = member_path(target) {
                    if let Some(id) = self.local(root) {
                        self.add(id, &fields, None);
                    }
                }
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }
}

struct Calls<'i, 'a, 's> {
    input: &'i FileInput<'a>,
    env: &'i ResourceEnv,
    locals: LocalDefs,
    regions: SideRegions,
    sink: &'i mut Sink<'s>,
}

impl<'ast> Visitor<'ast> for Calls<'_, '_, '_> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        match &expr.kind {
            ExprKind::Call { callee, args, .. } => {
                if let (Some((root, fields)), Some(display)) = (member_path(callee), callee.dotted_path()) {
                    let span = match &callee.kind {
                        ExprKind::Field { name, .. } => name.span,
                        ExprKind::Index { index, .. } => index.span,
                        _ => callee.span,
                    };
                    self.check_call(expr, root, &fields, args, false, span, &display);
                }
            }
            ExprKind::MethodCall { base, method, args, .. } => {
                if let (Some((root, mut fields)), Some(owner)) = (member_path(base), base.dotted_path()) {
                    fields.push(&method.text);
                    self.check_call(expr, root, &fields, args, true, method.span, &format!("{owner}:{}", method.text));
                }
            }
            _ => {}
        }
        visit::walk_expr(self, expr);
    }
}

impl Calls<'_, '_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn check_call(
        &mut self,
        call: &Expr,
        root: &Name,
        fields: &[&str],
        args: &[Expr],
        via_colon: bool,
        span: Span,
        display: &str,
    ) {
        let Some(passed) = passed_count(args, 0) else { return };
        let defs: Vec<Option<&Arc<FunType>>> = match self.input.resolution.resolve_at(root.span.start) {
            Some(Resolved::Local(id)) => {
                // Parameters and loop variables start out with a value this file does not know.
                let kind = self.input.resolution.local(id).kind;
                if !matches!(kind, LocalKind::Local | LocalKind::LocalFunction) {
                    return;
                }
                match self.locals.get(&(id, SmolStr::new(fields.join(".")))) {
                    Some(defs) => defs.iter().map(Option::as_ref).collect(),
                    None => return,
                }
            }
            // An encrypted script of the resource may define the global differently.
            Some(Resolved::Global(_)) if self.env.opaque => return,
            Some(Resolved::Global(_)) => {
                let Some(path) = global_key(&root.text, fields) else { return };
                let side = self.regions.effective(call.span.start, self.input.side);
                self.env.function_defs(&path, side).collect()
            }
            None => return,
        };

        // Any definition or overload that accepts fewer arguments may be the one that runs.
        let env = self.env;
        let alias = |name: &str| env.alias(name);
        let mut least: Option<Requirement> = None;
        for def in defs {
            let Some(fun) = def else { return };
            for candidate in std::iter::once(fun).chain(&fun.overloads) {
                let requirement = Requirement::of(candidate, via_colon, &alias);
                if least.as_ref().is_none_or(|least| requirement.arguments < least.arguments) {
                    least = Some(requirement);
                }
            }
        }
        let Some(least) = least.filter(|least| passed < least.arguments) else { return };
        let missing = match least.first_missing(passed) {
            Some(param) => format!("'{}' ({})", param.name, param.ty),
            None => "'self' (call it with ':')".to_string(),
        };
        self.sink.report(
            rules::MISSING_PARAMETER,
            span,
            format!(
                "'{display}' is called with {passed} argument{}, but needs {}; {missing} will be nil",
                plural(passed),
                least.arguments
            ),
        );
    }
}
