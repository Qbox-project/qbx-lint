use qbx_lua_syntax::ast::Attrib;

use super::{FileInput, Sink};
use crate::diagnostic::Tag;
use crate::rules;
use crate::scope::{Local, LocalKind};

pub(super) fn check(input: &FileInput, sink: &mut Sink) {
    let prefix = input.config.ignore_unused_prefix.as_str();
    for local in &input.resolution.locals {
        if local.name.is_empty() {
            continue;
        }
        check_const_writes(local, sink);
        let ignored = local.name == "_" || (!prefix.is_empty() && local.name.starts_with(prefix));
        if ignored || local.kind == LocalKind::ImplicitSelf {
            continue;
        }
        check_unused(local, sink);
        if let Some(previous) = local.redefines {
            let previous = input.resolution.local(previous);
            if previous.kind != LocalKind::Param || local.kind != LocalKind::Param {
                sink.report(
                    rules::REDEFINED_LOCAL,
                    local.decl,
                    format!("local '{}' is already declared in this scope", local.name),
                );
            }
        } else if local.shadows.is_some() {
            sink.report(rules::SHADOWED_LOCAL, local.decl, format!("local '{}' shadows an outer local", local.name));
        }
    }
}

fn check_const_writes(local: &Local, sink: &mut Sink) {
    let Some(attrib @ (Attrib::Const | Attrib::Close)) = local.attrib else { return };
    let label = if attrib == Attrib::Const { "const" } else { "close" };
    for write in local.refs.iter().filter(|r| r.write) {
        sink.report(
            rules::CONST_REASSIGN,
            write.span,
            format!("cannot assign to '{}': it is declared <{label}>", local.name),
        );
    }
}

fn check_unused(local: &Local, sink: &mut Sink) {
    if local.is_read() || local.attrib == Some(Attrib::Close) {
        return;
    }
    let name = &local.name;
    let (code, message) = match local.kind {
        LocalKind::Param => (rules::UNUSED_ARGUMENT, format!("unused argument '{name}'")),
        LocalKind::LoopVar => (rules::UNUSED_LOOP_VARIABLE, format!("unused loop variable '{name}'")),
        LocalKind::LocalFunction => (rules::UNUSED_FUNCTION, format!("unused function '{name}'")),
        LocalKind::Local if local.refs.iter().any(|r| r.write) => {
            (rules::UNUSED_LOCAL, format!("local '{name}' is assigned but never read"))
        }
        LocalKind::Local => (rules::UNUSED_LOCAL, format!("unused local '{name}'")),
        LocalKind::ImplicitSelf => return,
    };
    sink.report_with(code, local.decl, message, Some(Tag::Unnecessary), None);
}
