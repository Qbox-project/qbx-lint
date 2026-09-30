//! `no-unknown`: a parameter, local or loop variable that has no type, because none is declared
//! and none can be inferred. The rule is off until a project turns it on, since most code is not
//! annotated this far.

use qbx_lua_analysis::scope::{Local, LocalId, LocalKind};
use qbx_lua_syntax::Span;

use crate::infer::{Decl, Infer};
use crate::types::Type;

/// The declaration of each local name whose type is unknown, with a message naming what gives it
/// one. `self`, local functions and the names that start with `ignored_prefix`, as unused locals
/// do, are left out.
pub fn unknown_types(infer: &Infer, ignored_prefix: &str) -> Vec<(Span, String)> {
    let mut out = Vec::new();
    for (id, local) in infer.ctx.resolution.locals.iter().enumerate() {
        let name = local.name.as_str();
        let ignored = name == "_" || (!ignored_prefix.is_empty() && name.starts_with(ignored_prefix));
        if name.is_empty() || name == "self" || ignored {
            continue;
        }
        if matches!(local.kind, LocalKind::ImplicitSelf | LocalKind::LocalFunction) {
            continue;
        }
        let known = match infer.local_type(id as LocalId) {
            Type::Unknown => false,
            // The handler of `RegisterNetEvent(name, function(payload) end)` takes `any` from the
            // `fun(...)` it is passed as, which says no more about `payload` than nothing does.
            Type::Any if local.kind == LocalKind::Param => has_param_line(infer, local),
            _ => true,
        };
        if known {
            continue;
        }
        let message = match local.kind {
            LocalKind::Param => format!("Parameter `{name}` has no type; add `---@param {name} <type>`"),
            LocalKind::LoopVar => {
                format!("Loop variable `{name}` has no type; give the value the loop goes through one")
            }
            _ => format!("The type of `{name}` is unknown; add `---@type <type>`"),
        };
        out.push((local.decl, message));
    }
    out
}

/// Whether a `@param` line documents the parameter: above its function, or above the call its
/// function is passed to.
pub(super) fn has_param_line(infer: &Infer, param: &Local) -> bool {
    let Some(Decl::Param { doc_anchor, expected, .. }) = infer.ctx.decl(param.decl.start) else { return false };
    let anchor = doc_anchor.or_else(|| expected.as_ref().map(|expected| expected.call.span.start));
    anchor.is_some_and(|anchor| infer.ctx.doc_at(anchor).params.iter().any(|doc| doc.name == param.name))
}
