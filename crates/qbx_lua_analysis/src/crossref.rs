use qbx_fivem_data::Side;
use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::SmolStr;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arity {
    pub params: usize,
    pub vararg: bool,
}

impl Arity {
    pub fn of(func: &FuncBody) -> Self {
        Self { params: func.params.len(), vararg: func.vararg.is_some() }
    }
}

#[derive(Clone, Debug)]
pub struct EventRegistration {
    pub side: Option<Side>,
    pub handler: Option<Arity>,
}

/// Facts that one file needs to know about every other file: who handles which event and what
/// each resource exports. The linter fills it from the files it parsed, the language server from
/// its index.
#[derive(Clone, Debug, Default)]
pub struct CrossRefs {
    pub events: FxHashMap<SmolStr, Vec<EventRegistration>>,
    pub exports: FxHashMap<(SmolStr, SmolStr), Arity>,
    /// Resources whose Lua files were seen, so a missing export really is missing.
    pub resources: FxHashSet<SmolStr>,
    /// Resources with encrypted or unreadable scripts: what they export and handle beyond the
    /// readable part is unknown, so nothing may be called "missing" there.
    pub opaque_resources: FxHashSet<SmolStr>,
}

pub const EVENT_REGISTRATION_CALLS: &[&str] = &["RegisterNetEvent", "RegisterServerEvent", "AddEventHandler"];

impl CrossRefs {
    pub fn add_event(&mut self, name: SmolStr, side: Option<Side>, handler: Option<Arity>) {
        self.events.entry(name).or_default().push(EventRegistration { side, handler });
    }

    pub fn collect(&mut self, chunk: &Chunk, side: Option<Side>, resource: Option<&str>) {
        if let Some(resource) = resource {
            self.resources.insert(SmolStr::new(resource));
        }
        let mut collector = Collector { refs: self, side, resource, functions: FxHashMap::default() };
        collector.visit_block(&chunk.block);
    }
}

struct Collector<'a> {
    refs: &'a mut CrossRefs,
    side: Option<Side>,
    resource: Option<&'a str>,
    functions: FxHashMap<SmolStr, Arity>,
}

impl<'ast> Visitor<'ast> for Collector<'_> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match &stmt.kind {
            StmtKind::LocalFunction { name, func } => {
                self.functions.insert(name.text.clone(), Arity::of(func));
            }
            StmtKind::Function { name, func } if name.path.is_empty() && name.method.is_none() => {
                self.functions.insert(name.base.text.clone(), Arity::of(func));
            }
            StmtKind::Local { names, exprs, .. } => {
                for (name, expr) in names.iter().zip(exprs) {
                    if let ExprKind::Function(func) = &expr.kind {
                        self.functions.insert(name.name.text.clone(), Arity::of(func));
                    }
                }
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        if let ExprKind::Call { callee, args, .. } = &expr.kind {
            let path = callee.dotted_path();
            let name = args.first().and_then(Expr::as_string);
            match (path.as_deref(), name) {
                (Some(call), Some(name)) if EVENT_REGISTRATION_CALLS.contains(&call) => {
                    let handler = args.iter().skip(1).find_map(|arg| self.arity_of(arg));
                    self.refs.add_event(name.clone(), self.side, handler);
                }
                (Some("exports"), Some(name)) => {
                    if let (Some(resource), Some(arity)) = (self.resource, args.get(1).and_then(|a| self.arity_of(a))) {
                        self.refs.exports.insert((SmolStr::new(resource), name.clone()), arity);
                    } else if let Some(resource) = self.resource {
                        let unknown = Arity { params: 0, vararg: true };
                        self.refs.exports.insert((SmolStr::new(resource), name.clone()), unknown);
                    }
                }
                // `exports(name, fn)` in a loop (ox_lib does this): the export names only exist at
                // runtime, so the literal ones are not the whole list.
                (Some("exports"), None) if !args.is_empty() => {
                    if let Some(resource) = self.resource {
                        self.refs.opaque_resources.insert(SmolStr::new(resource));
                    }
                }
                _ => {}
            }
        }
        visit::walk_expr(self, expr);
    }
}

impl Collector<'_> {
    fn arity_of(&self, expr: &Expr) -> Option<Arity> {
        match &expr.kind {
            ExprKind::Function(func) => Some(Arity::of(func)),
            ExprKind::Name(name) => self.functions.get(&name.text).copied(),
            _ => None,
        }
    }
}

/// Where a trigger call delivers its event and how many leading arguments are not payload.
pub fn trigger_target(call: &str, own_side: Option<Side>) -> Option<(Option<Side>, usize)> {
    Some(match call {
        "TriggerServerEvent" => (Some(Side::Server), 1),
        "TriggerLatentServerEvent" => (Some(Side::Server), 2),
        "TriggerClientEvent" => (Some(Side::Client), 2),
        "TriggerLatentClientEvent" => (Some(Side::Client), 3),
        "TriggerEvent" => (own_side.filter(|s| *s != Side::Shared), 1),
        _ => return None,
    })
}

impl CrossRefs {
    pub fn merge(&mut self, other: CrossRefs) {
        for (name, registrations) in other.events {
            self.events.entry(name).or_default().extend(registrations);
        }
        self.exports.extend(other.exports);
        self.resources.extend(other.resources);
        self.opaque_resources.extend(other.opaque_resources);
    }
}
