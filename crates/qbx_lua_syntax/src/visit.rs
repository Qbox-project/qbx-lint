use crate::ast::*;

pub trait Visitor<'ast>: Sized {
    fn visit_block(&mut self, block: &'ast Block) {
        walk_block(self, block);
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        walk_expr(self, expr);
    }

    fn visit_func_body(&mut self, func: &'ast FuncBody) {
        walk_func_body(self, func);
    }
}

pub fn walk_block<'ast, V: Visitor<'ast>>(v: &mut V, block: &'ast Block) {
    for stmt in &block.stmts {
        v.visit_stmt(stmt);
    }
}

pub fn walk_func_body<'ast, V: Visitor<'ast>>(v: &mut V, func: &'ast FuncBody) {
    v.visit_block(&func.body);
}

pub fn walk_stmt<'ast, V: Visitor<'ast>>(v: &mut V, stmt: &'ast Stmt) {
    match &stmt.kind {
        StmtKind::Local { exprs, .. } => exprs.iter().for_each(|e| v.visit_expr(e)),
        StmtKind::LocalFunction { func, .. } | StmtKind::Function { func, .. } => v.visit_func_body(func),
        StmtKind::Assign { targets, exprs } => {
            targets.iter().for_each(|e| v.visit_expr(e));
            exprs.iter().for_each(|e| v.visit_expr(e));
        }
        StmtKind::CompoundAssign { target, expr, .. } => {
            v.visit_expr(target);
            v.visit_expr(expr);
        }
        StmtKind::Expr(expr) => v.visit_expr(expr),
        StmtKind::Do(body) | StmtKind::Defer(body) => v.visit_block(body),
        StmtKind::While { cond, body } => {
            v.visit_expr(cond);
            v.visit_block(body);
        }
        StmtKind::Repeat { body, cond } => {
            v.visit_block(body);
            v.visit_expr(cond);
        }
        StmtKind::If { branches, else_block } => {
            for branch in branches {
                v.visit_expr(&branch.cond);
                v.visit_block(&branch.block);
            }
            if let Some(block) = else_block {
                v.visit_block(block);
            }
        }
        StmtKind::NumericFor { start, limit, step, body, .. } => {
            v.visit_expr(start);
            v.visit_expr(limit);
            if let Some(step) = step {
                v.visit_expr(step);
            }
            v.visit_block(body);
        }
        StmtKind::GenericFor { exprs, body, .. } => {
            exprs.iter().for_each(|e| v.visit_expr(e));
            v.visit_block(body);
        }
        StmtKind::Return(exprs) => exprs.iter().for_each(|e| v.visit_expr(e)),
        StmtKind::Break | StmtKind::Goto(_) | StmtKind::Label(_) | StmtKind::Error => {}
    }
}

pub fn walk_expr<'ast, V: Visitor<'ast>>(v: &mut V, expr: &'ast Expr) {
    match &expr.kind {
        ExprKind::Function(func) => v.visit_func_body(func),
        ExprKind::Index { base, index, .. } => {
            v.visit_expr(base);
            v.visit_expr(index);
        }
        ExprKind::Field { base, .. } => v.visit_expr(base),
        ExprKind::Call { callee, args, .. } => {
            v.visit_expr(callee);
            args.iter().for_each(|e| v.visit_expr(e));
        }
        ExprKind::MethodCall { base, args, .. } => {
            v.visit_expr(base);
            args.iter().for_each(|e| v.visit_expr(e));
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            v.visit_expr(lhs);
            v.visit_expr(rhs);
        }
        ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) => v.visit_expr(expr),
        ExprKind::Table(fields) => {
            for field in fields {
                match field {
                    TableField::Positional(value) | TableField::Named { value, .. } => v.visit_expr(value),
                    TableField::Keyed { key, value } => {
                        v.visit_expr(key);
                        v.visit_expr(value);
                    }
                    TableField::SetMember(_) => {}
                }
            }
        }
        ExprKind::Nil
        | ExprKind::True
        | ExprKind::False
        | ExprKind::Vararg
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::JenkinsHash(_)
        | ExprKind::Name(_)
        | ExprKind::Error => {}
    }
}
