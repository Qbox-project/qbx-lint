use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::{SmolStr, Span};

use crate::scope::{GlobalRefKind, Resolution, Resolved, MAIN_CHUNK};

#[derive(Clone, Debug)]
pub struct GlobalDef {
    pub name: SmolStr,
    pub span: Span,
    pub is_function: bool,
    pub at_file_scope: bool,
}

/// What a file contributes to the shared global environment of its resource.
#[derive(Clone, Debug, Default)]
pub struct FileSummary {
    pub global_defs: Vec<GlobalDef>,
    /// `(table, field)` pairs assigned on global tables, e.g. `function table.contains() end`.
    pub global_field_defs: Vec<(SmolStr, SmolStr)>,
}

pub fn summarize(chunk: &Chunk, resolution: &Resolution) -> FileSummary {
    let global_defs = resolution
        .globals
        .iter()
        .filter(|g| g.is_definition())
        .map(|g| GlobalDef {
            name: g.name.clone(),
            span: g.span,
            is_function: g.kind == GlobalRefKind::FunctionDecl,
            at_file_scope: g.func == MAIN_CHUNK,
        })
        .collect();
    let mut collector = FieldDefs { resolution, out: Vec::new(), env_defs: Vec::new(), depth: 0 };
    collector.visit_block(&chunk.block);
    let mut global_defs: Vec<GlobalDef> = global_defs;
    global_defs.extend(collector.env_defs);
    FileSummary { global_defs, global_field_defs: collector.out }
}

struct FieldDefs<'a> {
    resolution: &'a Resolution,
    out: Vec<(SmolStr, SmolStr)>,
    env_defs: Vec<GlobalDef>,
    depth: u32,
}

impl FieldDefs<'_> {
    fn is_global(&self, name: &Name) -> bool {
        matches!(self.resolution.resolve_at(name.span.start), Some(Resolved::Global(_)))
    }
}

impl Visitor for FieldDefs<'_> {
    fn visit_func_body(&mut self, func: &FuncBody) {
        self.depth += 1;
        visit::walk_func_body(self, func);
        self.depth -= 1;
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Function { name, .. } if self.is_global(&name.base) => {
                if let Some(member) = name.path.first().or(name.method.as_ref()) {
                    self.out.push((name.base.text.clone(), member.text.clone()));
                }
            }
            StmtKind::Assign { targets, .. } => {
                for target in targets {
                    if let ExprKind::Field { base, name, .. } = &target.kind {
                        if let ExprKind::Name(base) = &base.kind {
                            if self.is_global(base) && matches!(base.text.as_str(), "_ENV" | "_G") {
                                self.env_defs.push(GlobalDef {
                                    name: name.text.clone(),
                                    span: name.span,
                                    is_function: false,
                                    at_file_scope: self.depth == 0,
                                });
                            } else if self.is_global(base) {
                                self.out.push((base.text.clone(), name.text.clone()));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }
}
