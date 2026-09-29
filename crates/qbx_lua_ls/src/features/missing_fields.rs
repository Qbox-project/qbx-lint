//! `missing-fields`: a table constructor whose type is a LuaCATS class leaves out fields the class
//! requires. Only the language server knows the classes, so qbx-lint registers the rule and this
//! module reports it.

use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::{SmolStr, Span};
use rustc_hash::FxHashSet;

use crate::index::{ClassDef, ResourceId};
use crate::infer::Infer;
use crate::luacats::applies_on;
use crate::types::Type;

const MAX_DEPTH: u32 = 8;

/// Each table constructor that lacks required fields, with the message naming them.
pub fn missing_fields(infer: &Infer, chunk: &Chunk) -> Vec<(Span, String)> {
    let mut check = Check { infer, out: Vec::new() };
    check.visit_block(&chunk.block);
    check.out
}

struct Check<'a, 'b> {
    infer: &'a Infer<'b>,
    out: Vec<(Span, String)>,
}

fn is_table(expr: &Expr) -> bool {
    matches!(expr.unparen().kind, ExprKind::Table(_))
}

/// A field a constructor sets by name, `name = v`, `['name'] = v` or `.name`, with its value.
fn named_field(field: &TableField) -> Option<(&str, Option<&Expr>)> {
    match field {
        TableField::Named { name, value } => Some((name.text.as_str(), Some(value))),
        TableField::Keyed { key, value } => key.as_string().map(|name| (name.as_str(), Some(value))),
        TableField::SetMember(name) => Some((name.text.as_str(), None)),
        TableField::Positional(_) => None,
    }
}

impl<'b> Check<'_, 'b> {
    /// Checks `expr` against `expected` when it is a table constructor and `expected` is a class,
    /// then the constructors it holds for fields that are classes too.
    fn check(&mut self, expected: &Type, expr: &Expr, depth: u32) {
        let ExprKind::Table(fields) = &expr.unparen().kind else { return };
        let Some(class) = self.class_of(expected) else { return };
        let declared = self.fields_of(&class, 0);
        let given: Vec<(&str, Option<&Expr>)> = fields.iter().filter_map(named_field).collect();
        let missing: Vec<String> = declared
            .iter()
            .filter(|(name, ty)| !self.admits_nil(ty, 0) && !given.iter().any(|(given, _)| *given == name.as_str()))
            .map(|(name, _)| format!("`{name}`"))
            .collect();
        if !missing.is_empty() {
            let message = format!("Missing required fields in type `{class}`: {}", missing.join(", "));
            self.out.push((expr.span, message));
        }
        if depth < MAX_DEPTH {
            for (name, value) in given {
                let field = declared.iter().find(|(declared, _)| declared.as_str() == name);
                if let (Some((_, ty)), Some(value)) = (field, value) {
                    self.check(ty, value, depth + 1);
                }
            }
        }
    }

    /// The class a value of type `ty` must be, looking through `?` and aliases.
    fn class_of(&self, ty: &Type) -> Option<SmolStr> {
        match self.infer.resolve_alias(&ty.without_nil()).without_nil() {
            Type::Named(name, _) if !self.class_defs(&name).is_empty() => Some(name),
            _ => None,
        }
    }

    /// The declarations of class `name` this file can see. When it sees none, a class that several
    /// resources declare under one name is ambiguous and is not checked.
    fn class_defs(&self, name: &str) -> Vec<&'b ClassDef> {
        let index = self.infer.index;
        let from = self.infer.ctx.file;
        let mut defs = index.class_defs(name);
        defs.retain(|(_, class)| applies_on(class.side, self.infer.side()));
        let visible: Vec<&ClassDef> =
            defs.iter().filter(|(file, _)| index.is_visible(from, *file)).map(|(_, class)| *class).collect();
        if !visible.is_empty() {
            return visible;
        }
        let resources: FxHashSet<Option<ResourceId>> =
            defs.iter().map(|(file, _)| index.file(*file).and_then(|f| f.resource)).collect();
        if resources.len() == 1 {
            defs.into_iter().map(|(_, class)| class).collect()
        } else {
            Vec::new()
        }
    }

    /// The `@field`s of `class` and its parents, leaving out those scoped to the other side. The
    /// first declaration of a name wins, so a class can narrow a field of its parent.
    fn fields_of(&self, class: &str, depth: u32) -> Vec<(SmolStr, Type)> {
        let mut out: Vec<(SmolStr, Type)> = Vec::new();
        if depth > MAX_DEPTH {
            return out;
        }
        let defs = self.class_defs(class);
        for def in &defs {
            let sides = def.field_sides.iter().copied().chain(std::iter::repeat(None));
            for (field, side) in def.fields.iter().zip(sides) {
                if applies_on(side, self.infer.side()) && !out.iter().any(|(name, _)| *name == field.name) {
                    out.push((field.name.clone(), field.ty.clone()));
                }
            }
        }
        for parent in defs.iter().flat_map(|def| &def.parents) {
            for (name, ty) in self.fields_of(parent, depth + 1) {
                if !out.iter().any(|(seen, _)| *seen == name) {
                    out.push((name, ty));
                }
            }
        }
        out
    }

    /// Whether a field of type `ty` may be left out: `string?`, `string|nil`, `any` or no type.
    fn admits_nil(&self, ty: &Type, depth: u32) -> bool {
        if depth > MAX_DEPTH {
            return true;
        }
        match self.infer.resolve_alias(ty) {
            Type::Nil | Type::Any | Type::Unknown => true,
            Type::Union(types) => types.iter().any(|t| self.admits_nil(t, depth + 1)),
            _ => false,
        }
    }
}

impl<'ast> Visitor<'ast> for Check<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match &stmt.kind {
            StmtKind::Local { exprs, .. } if exprs.iter().any(is_table) => {
                let doc = self.infer.ctx.doc_at(stmt.span.start);
                // `---@class Name` above a table declares the class rather than an instance of it.
                if let (true, Some(ty)) = (doc.classes.is_empty(), &doc.ty) {
                    for expr in exprs {
                        self.check(ty, expr, 0);
                    }
                }
            }
            StmtKind::Assign { targets, exprs } if exprs.iter().any(is_table) => {
                let doc = self.infer.ctx.doc_at(stmt.span.start);
                if doc.classes.is_empty() {
                    for (target, expr) in targets.iter().zip(exprs).filter(|(_, expr)| is_table(expr)) {
                        let expected = doc.ty.clone().unwrap_or_else(|| self.infer.expr(target));
                        self.check(&expected, expr, 0);
                    }
                }
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        let call = match &expr.kind {
            ExprKind::Call { callee, args, .. } => Some((&**callee, None, args.as_slice())),
            ExprKind::MethodCall { base, method, args, .. } => Some((&**base, Some(method), args.as_slice())),
            _ => None,
        };
        if let Some((base, method, args)) = call.filter(|(_, _, args)| args.iter().any(is_table)) {
            if let Some((fun, _)) = self.infer.callee_fun(base, method) {
                let (skip_params, skip_args) = fun.call_offsets(method.is_some());
                for (param, arg) in fun.params.iter().skip(skip_params).zip(args.iter().skip(skip_args)) {
                    self.check(&param.ty, arg, 0);
                }
            }
        }
        visit::walk_expr(self, expr);
    }
}
