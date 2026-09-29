//! `missing-parameter` for the payload of a `---@callback await` or `trigger` wrapper call: the
//! values passed in its `...` go to the handler registered under the name it passes, so they have
//! to cover that handler's required parameters, after the player a handler outside the client
//! receives first. qbx-lint checks the wrapper's own parameters, but only the language server
//! knows which handler a name reaches.

use qbx_lua_analysis::signature::Requirement;
use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::Span;

use crate::callback_wrappers::{self, Wrapper};
use crate::index::{EventFamily, EventKind};
use crate::infer::Infer;
use crate::types::{CallbackRole, FunType};

/// Each call to an `await` or `trigger` wrapper whose payload leaves out a parameter the handler
/// requires, with the message naming it.
pub fn missing_payloads(infer: &Infer, chunk: &Chunk) -> Vec<(Span, String)> {
    let has_handlers = infer.index.events().any(|(_, event)| {
        matches!(event.family, EventFamily::Custom(_)) && event.kind == EventKind::Callback && event.handler.is_some()
    });
    if !has_handlers {
        return Vec::new();
    }
    let mut calls = Calls { infer, out: Vec::new() };
    calls.visit_block(&chunk.block);
    calls.out
}

struct Calls<'a, 'b> {
    infer: &'a Infer<'b>,
    out: Vec<(Span, String)>,
}

impl<'ast> Visitor<'ast> for Calls<'_, '_> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        match &expr.kind {
            ExprKind::Call { callee, args, .. } => {
                let span = match &callee.kind {
                    ExprKind::Field { name, .. } => name.span,
                    ExprKind::Index { index, .. } => index.span,
                    _ => callee.span,
                };
                self.check(expr, callee, None, args, span);
            }
            ExprKind::MethodCall { base, method, args, .. } => self.check(expr, base, Some(method), args, method.span),
            _ => {}
        }
        visit::walk_expr(self, expr);
    }
}

impl Calls<'_, '_> {
    fn check(&mut self, call: &Expr, base: &Expr, method: Option<&Name>, args: &[Expr], span: Span) {
        // Without a literal name there is no handler to find, and most calls pass none.
        if !args.iter().any(|arg| arg.as_string().is_some()) {
            return;
        }
        let Some((fun, _)) = self.infer.callee_fun(base, method) else { return };
        let Some(wrapper) = Wrapper::of(&fun, method.is_some()) else { return };
        let Some(payload) = wrapper.payload.filter(|_| wrapper.tag.role != CallbackRole::Register) else { return };
        let Some(name) = args.get(wrapper.arg(wrapper.name)).and_then(Expr::as_string) else { return };
        // A call or `...` at the end passes as many values as it gives.
        let Some(passed) =
            args.get(wrapper.arg(payload)..).filter(|rest| !rest.last().is_some_and(Expr::is_multi_value))
        else {
            return;
        };
        let passed = passed.len();

        let target = callback_wrappers::target_of(self.infer.side_at(call.span.start));
        let payloads: Vec<FunType> = callback_wrappers::handlers(self.infer.index, &wrapper.family(), name, target)
            .into_iter()
            .filter_map(|(_, event)| {
                let handler = event.handler.as_deref()?;
                let params = handler.params[callback_wrappers::source_skip(event, handler)..].to_vec();
                Some(FunType { params, ..FunType::default() })
            })
            .collect();
        // Any of the handlers may be the one that runs, so the one that needs the fewest decides.
        let alias = |name: &str| self.infer.index.alias(name, target).map(|(_, alias)| &alias.ty);
        let Some(least) = payloads
            .iter()
            .map(|payload| Requirement::of(payload, false, &alias))
            .min_by_key(|requirement| requirement.arguments)
            .filter(|least| passed < least.arguments)
        else {
            return;
        };
        let Some(param) = least.first_missing(passed) else { return };
        let message = format!(
            "Callback '{name}' is called with {passed} argument{}, but needs {}; '{}' ({}) will be nil",
            if passed == 1 { "" } else { "s" },
            least.arguments,
            param.name,
            param.ty
        );
        self.out.push((span, message));
    }
}
