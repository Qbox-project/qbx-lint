//! Table constructors typed as a LuaCATS class, found where their type is known: `---@type` locals
//! and assignments, arguments to class-typed parameters, the tables such fields hold and, for
//! completion, `return` in a function documented with `@return`. They drive `missing-fields` and the
//! completion of field names. Only the language server knows the classes, so qbx-lint registers
//! the rules and this module reports them.

use std::sync::Arc;

use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::visit::{self, Visitor};
use qbx_lua_syntax::{SmolStr, Span};
use rustc_hash::FxHashSet;

use crate::index::{ClassDef, ResourceId};
use crate::infer::Infer;
use crate::luacats::applies_on;
use crate::types::Type;

const MAX_DEPTH: u32 = 8;

/// A table constructor and the class it has to be.
pub struct ClassTable<'c> {
    pub class: SmolStr,
    pub table: &'c Expr,
}

/// A `@field` of a class, or of one of its parents.
pub struct ClassField {
    pub name: SmolStr,
    pub ty: Type,
    pub doc: Option<Arc<str>>,
}

/// A field a constructor sets by name, `name = v`, `['name'] = v` or `.name`, with its value.
pub fn named_field(field: &TableField) -> Option<(&str, Option<&Expr>)> {
    match field {
        TableField::Named { name, value } => Some((name.text.as_str(), Some(value))),
        TableField::Keyed { key, value } => key.as_string().map(|name| (name.as_str(), Some(value))),
        TableField::SetMember(name) => Some((name.text.as_str(), None)),
        TableField::Positional(_) => None,
    }
}

fn is_table(expr: &Expr) -> bool {
    matches!(expr.unparen().kind, ExprKind::Table(_))
}

/// Looks up the classes a file can see.
pub struct Classes<'a, 'b> {
    infer: &'a Infer<'b>,
}

impl<'a, 'b> Classes<'a, 'b> {
    pub fn new(infer: &'a Infer<'b>) -> Self {
        Self { infer }
    }

    /// The class a value of type `ty` must be, looking through `?` and aliases.
    pub fn class_of(&self, ty: &Type) -> Option<SmolStr> {
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
    pub fn fields(&self, class: &str) -> Vec<ClassField> {
        let mut out = Vec::new();
        self.collect_fields(class, &mut out, 0);
        out
    }

    fn collect_fields(&self, class: &str, out: &mut Vec<ClassField>, depth: u32) {
        if depth > MAX_DEPTH {
            return;
        }
        let defs = self.class_defs(class);
        for def in &defs {
            let sides = def.field_sides.iter().copied().chain(std::iter::repeat(None));
            for (field, side) in def.fields.iter().zip(sides) {
                if applies_on(side, self.infer.side()) && !out.iter().any(|seen| seen.name == field.name) {
                    out.push(ClassField { name: field.name.clone(), ty: field.ty.clone(), doc: field.doc.clone() });
                }
            }
        }
        for parent in defs.iter().flat_map(|def| &def.parents) {
            self.collect_fields(parent, out, depth + 1);
        }
    }

    /// Whether a field of type `ty` may be left out: `string?`, `string|nil`, `any` or no type.
    pub fn admits_nil(&self, ty: &Type) -> bool {
        self.admits_nil_at(ty, 0)
    }

    fn admits_nil_at(&self, ty: &Type, depth: u32) -> bool {
        if depth > MAX_DEPTH {
            return true;
        }
        match self.infer.resolve_alias(ty) {
            Type::Nil | Type::Any | Type::Unknown => true,
            Type::Union(types) => types.iter().any(|t| self.admits_nil_at(t, depth + 1)),
            _ => false,
        }
    }
}

/// Every class-typed table constructor of `chunk`, outer tables before the ones they hold. `returns`
/// also takes `return { ... }` in functions documented with `@return`.
pub fn class_tables<'c>(infer: &Infer, chunk: &'c Chunk, returns: bool) -> Vec<ClassTable<'c>> {
    let mut finder = Finder {
        classes: Classes::new(infer),
        returns,
        function_returns: Vec::new(),
        next_returns: None,
        out: Vec::new(),
    };
    finder.visit_block(&chunk.block);
    finder.out
}

/// The class-typed table constructor directly around `offset`, for completing its field names. A
/// table nested in it that is not typed as a class is not its own.
pub fn class_table_at<'c>(infer: &Infer, chunk: &'c Chunk, offset: u32) -> Option<ClassTable<'c>> {
    let mut innermost = Innermost { offset, found: None };
    innermost.visit_block(&chunk.block);
    let span = innermost.found?;
    class_tables(infer, chunk, true).into_iter().find(|found| found.table.span == span)
}

/// The innermost table constructor whose braces hold `offset`.
struct Innermost {
    offset: u32,
    found: Option<Span>,
}

impl<'c> Visitor<'c> for Innermost {
    fn visit_expr(&mut self, expr: &'c Expr) {
        if !expr.span.contains_inclusive(self.offset) {
            return;
        }
        if matches!(expr.kind, ExprKind::Table(_)) && expr.span.start < self.offset && self.offset < expr.span.end {
            self.found = Some(expr.span);
        }
        visit::walk_expr(self, expr);
    }
}

/// Each class-typed table constructor that leaves out required fields, with the message naming them.
pub fn missing_fields(infer: &Infer, chunk: &Chunk) -> Vec<(Span, String)> {
    let classes = Classes::new(infer);
    class_tables(infer, chunk, false)
        .into_iter()
        .filter_map(|found| {
            let ExprKind::Table(fields) = &found.table.kind else { return None };
            let given: Vec<&str> = fields.iter().filter_map(named_field).map(|(name, _)| name).collect();
            let missing: Vec<String> = classes
                .fields(&found.class)
                .iter()
                .filter(|field| !classes.admits_nil(&field.ty) && !given.contains(&field.name.as_str()))
                .map(|field| format!("`{}`", field.name))
                .collect();
            let message = format!("Missing required fields in type `{}`: {}", found.class, missing.join(", "));
            (!missing.is_empty()).then_some((found.table.span, message))
        })
        .collect()
}

struct Finder<'a, 'b, 'c> {
    classes: Classes<'a, 'b>,
    returns: bool,
    /// The `@return` types of the functions around the statement being visited, innermost last.
    function_returns: Vec<Vec<Type>>,
    /// The `@return` types of the function whose body is visited next.
    next_returns: Option<Vec<Type>>,
    out: Vec<ClassTable<'c>>,
}

impl<'c> Finder<'_, '_, 'c> {
    /// Records `expr` when it is a table constructor and `expected` is a class, then the
    /// constructors it holds for fields that are classes too.
    fn table(&mut self, expected: &Type, expr: &'c Expr, depth: u32) {
        let table = expr.unparen();
        let ExprKind::Table(fields) = &table.kind else { return };
        let Some(class) = self.classes.class_of(expected) else { return };
        let declared = self.classes.fields(&class);
        self.out.push(ClassTable { class, table });
        if depth < MAX_DEPTH {
            for (name, value) in fields.iter().filter_map(named_field) {
                if let (Some(field), Some(value)) = (declared.iter().find(|field| field.name == name), value) {
                    self.table(&field.ty, value, depth + 1);
                }
            }
        }
    }

    fn doc_returns(&self, stmt: &Stmt) -> Vec<Type> {
        self.classes.infer.ctx.doc_at(stmt.span.start).returns.iter().map(|r| r.ty.clone()).collect()
    }
}

impl<'c> Visitor<'c> for Finder<'_, '_, 'c> {
    fn visit_stmt(&mut self, stmt: &'c Stmt) {
        match &stmt.kind {
            StmtKind::Local { exprs, .. } if exprs.iter().any(is_table) => {
                let doc = self.classes.infer.ctx.doc_at(stmt.span.start);
                // `---@class Name` above a table declares the class rather than an instance of it.
                if let (true, Some(ty)) = (doc.classes.is_empty(), &doc.ty) {
                    for expr in exprs {
                        self.table(ty, expr, 0);
                    }
                }
            }
            StmtKind::Assign { targets, exprs } if exprs.iter().any(is_table) => {
                let doc = self.classes.infer.ctx.doc_at(stmt.span.start);
                if doc.classes.is_empty() {
                    for (target, expr) in targets.iter().zip(exprs).filter(|(_, expr)| is_table(expr)) {
                        let expected = doc.ty.clone().unwrap_or_else(|| self.classes.infer.expr(target));
                        self.table(&expected, expr, 0);
                    }
                }
            }
            StmtKind::Return(exprs) if self.returns && exprs.iter().any(is_table) => {
                let expected = self.function_returns.last().cloned().unwrap_or_default();
                for (ty, expr) in expected.iter().zip(exprs) {
                    self.table(ty, expr, 0);
                }
            }
            StmtKind::Function { .. } | StmtKind::LocalFunction { .. } => {
                self.next_returns = Some(self.doc_returns(stmt));
            }
            // `local f = function() ... end` below its doc comment.
            StmtKind::Local { exprs, .. } if matches!(exprs.as_slice(), [Expr { kind: ExprKind::Function(_), .. }]) => {
                self.next_returns = Some(self.doc_returns(stmt));
            }
            _ => {}
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_func_body(&mut self, func: &'c FuncBody) {
        let returns = self.next_returns.take().unwrap_or_default();
        self.function_returns.push(returns);
        visit::walk_func_body(self, func);
        self.function_returns.pop();
    }

    fn visit_expr(&mut self, expr: &'c Expr) {
        let call = match &expr.kind {
            ExprKind::Call { callee, args, .. } => Some((&**callee, None, args.as_slice())),
            ExprKind::MethodCall { base, method, args, .. } => Some((&**base, Some(method), args.as_slice())),
            _ => None,
        };
        if let Some((base, method, args)) = call.filter(|(_, _, args)| args.iter().any(is_table)) {
            if let Some((fun, _)) = self.classes.infer.callee_fun(base, method) {
                let (skip_params, skip_args) = fun.call_offsets(method.is_some());
                for (param, arg) in fun.params.iter().skip(skip_params).zip(args.iter().skip(skip_args)) {
                    self.table(&param.ty, arg, 0);
                }
            }
        }
        visit::walk_expr(self, expr);
    }
}
