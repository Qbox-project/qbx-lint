use qbx_fivem_data::Side;
use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::SmolStr;
use rustc_hash::FxHashSet;

use super::{FileInput, Sink};
use crate::crossref::{trigger_target, Arity, CrossRefs};
use crate::rules;
use crate::scope::Resolved;

pub(super) fn check(input: &FileInput, sink: &mut Sink) {
    let mentions_resource_state = input.source.contains("GetResourceState");
    let mut checker = CrossFile { input, sink, reported_dependencies: FxHashSet::default(), mentions_resource_state };
    checker.visit_block(&input.chunk.block);
}

struct CrossFile<'a, 'b> {
    input: &'a FileInput<'a>,
    sink: &'a mut Sink<'b>,
    reported_dependencies: FxHashSet<SmolStr>,
    mentions_resource_state: bool,
}

fn passed_count(args: &[Expr], skip: usize) -> Option<usize> {
    let payload = args.get(skip..)?;
    match payload.last() {
        Some(last) if last.is_multi_value() => None,
        _ => Some(payload.len()),
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

impl CrossFile<'_, '_> {
    fn is_global(&self, name: &Name) -> bool {
        matches!(self.input.resolution.resolve_at(name.span.start), Some(Resolved::Global(_)))
    }

    fn trigger(&mut self, refs: &CrossRefs, call: &str, expr: &Expr, args: &[Expr]) {
        let Some((target, skip)) = trigger_target(call, self.input.side) else { return };
        let Some(name_arg) = args.first() else { return };
        let Some(name) = name_arg.as_string() else { return };
        let Some(registrations) = refs.events.get(name) else { return };

        let reaches = |side: Option<Side>| match (target, side) {
            (Some(target), Some(side)) => side.is_available_on(target),
            _ => true,
        };
        let reachable: Vec<_> = registrations.iter().filter(|r| reaches(r.side)).collect();
        if reachable.is_empty() {
            let (Some(target), Some(other)) = (target, registrations.iter().find_map(|r| r.side)) else { return };
            self.sink.report(
                rules::EVENT_WRONG_SIDE,
                name_arg.span,
                format!(
                    "'{name}' is only handled on the {}, but {call} delivers it to the {}",
                    other.label(),
                    target.label()
                ),
            );
            return;
        }

        let Some(passed) = passed_count(args, skip) else { return };
        let arities: Vec<Arity> = reachable.iter().filter_map(|r| r.handler).collect();
        if arities.is_empty() || arities.iter().any(|a| a.vararg) {
            return;
        }
        let most = arities.iter().map(|a| a.params).max().unwrap_or(0);
        let least = arities.iter().map(|a| a.params).min().unwrap_or(0);
        if passed > most {
            self.sink.report(
                rules::EVENT_ARGUMENT_COUNT,
                expr.span,
                format!(
                    "'{name}' is triggered with {passed} argument{}, but its handler only takes {most}",
                    plural(passed)
                ),
            );
        } else if passed < least {
            self.sink.report(
                rules::EVENT_MISSING_ARGUMENTS,
                expr.span,
                format!(
                    "'{name}' is triggered with {passed} argument{}, but its handler declares {least}; the rest will be nil",
                    plural(passed)
                ),
            );
        }
    }

    fn exported_resource<'e>(&self, base: &'e Expr) -> Option<(SmolStr, &'e Expr)> {
        let (root, resource) = match &base.kind {
            ExprKind::Field { base, name, .. } => (base, name.text.clone()),
            ExprKind::Index { base, index, .. } => (base, index.as_string()?.clone()),
            _ => return None,
        };
        match &root.kind {
            ExprKind::Name(name) if name.text == "exports" && self.is_global(name) => Some((resource, base)),
            _ => None,
        }
    }

    fn export_call(&mut self, expr: &Expr, base: &Expr, method: &Name, args: &[Expr]) {
        let Some((resource, resource_expr)) = self.exported_resource(base) else { return };
        let own = self.input.resource.map(|r| r.name);
        if own == Some(resource.as_str()) {
            return;
        }
        self.dependency(&resource, resource_expr);

        let Some(refs) = self.input.crossrefs else { return };
        if !refs.resources.contains(&resource) {
            return;
        }
        match refs.exports.get(&(resource.clone(), method.text.clone())) {
            Some(arity) if !arity.vararg => {
                if let Some(passed) = passed_count(args, 0).filter(|passed| *passed > arity.params) {
                    self.sink.report(
                        rules::EXPORT_ARGUMENT_COUNT,
                        expr.span,
                        format!(
                            "export '{}' of '{resource}' takes {} argument{}, but {passed} are passed",
                            method.text,
                            arity.params,
                            plural(arity.params)
                        ),
                    );
                }
            }
            Some(_) => {}
            None => {
                let has_any = refs.exports.keys().any(|(r, _)| *r == resource);
                if has_any && !method.is_missing() {
                    self.sink.report(
                        rules::UNKNOWN_EXPORT,
                        method.span,
                        format!("'{resource}' does not register an export named '{}'", method.text),
                    );
                }
            }
        }
    }

    fn dependency(&mut self, resource: &SmolStr, at: &Expr) {
        let Some(own) = self.input.resource else { return };
        let listed = own.manifest.dependencies.iter().any(|d| d.value.trim_start_matches('/') == resource.as_str());
        let imported = own.manifest.imports().any(|s| s.pattern.starts_with(&format!("@{resource}/")));
        // `if GetResourceState('x') == 'started'` marks an optional integration, not a hard dependency.
        let guarded = self.mentions_resource_state
            && self
                .input
                .source
                .split("GetResourceState")
                .skip(1)
                .any(|rest| rest.get(..resource.len() + 6).unwrap_or(rest).contains(resource.as_str()));
        if listed || imported || guarded || !self.reported_dependencies.insert(resource.clone()) {
            return;
        }
        self.sink.report(
            rules::MANIFEST_MISSING_DEPENDENCY,
            at.span,
            format!("'{resource}' is used but not listed under dependencies in fxmanifest.lua, so it may start after this resource"),
        );
    }
}

impl<'ast> Visitor<'ast> for CrossFile<'_, '_> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        match &expr.kind {
            ExprKind::Call { callee, args, .. } => {
                if let (Some(refs), ExprKind::Name(name)) = (self.input.crossrefs, &callee.kind) {
                    if self.is_global(name) {
                        self.trigger(refs, &name.text, expr, args);
                    }
                }
            }
            ExprKind::MethodCall { base, method, args, .. } => self.export_call(expr, base, method, args),
            _ => {}
        }
        visit::walk_expr(self, expr);
    }
}
