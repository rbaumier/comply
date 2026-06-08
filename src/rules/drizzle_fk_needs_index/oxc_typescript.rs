//! drizzle-fk-needs-index — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, Expression, FunctionBody, ObjectPropertyKind,
    PropertyKey, Statement,
};
use rustc_hash::FxHashSet;
use std::sync::Arc;

const TABLE_CTORS: &[&str] = &["pgTable", "mysqlTable", "sqliteTable"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if !TABLE_CTORS.contains(&id.name.as_str()) {
            return;
        }

        let Some(Argument::ObjectExpression(cols)) = call.arguments.get(1) else {
            return;
        };

        let mut fk_columns: Vec<(String, u32)> = Vec::new();
        for prop in &cols.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            let Some(name) = property_key_name(&p.key) else {
                continue;
            };
            if !expr_calls_references(&p.value) {
                continue;
            }
            // Best-effort diagnostic position: start of the column's value
            // expression so the message lands on the column body, not on
            // the property key.
            let loc = match &p.value {
                Expression::CallExpression(c) => c.span.start,
                _ => p.span.start,
            };
            fk_columns.push((name.to_string(), loc));
        }

        if fk_columns.is_empty() {
            return;
        }

        // Walk the 3rd arg (the extras callback) and collect any column
        // names that are covered by an explicit index declaration:
        //   index(...).on(t.col)           → col
        //   uniqueIndex(...).on(t.col)     → col
        //   primaryKey({ columns: [t.a] }) → a (leading column only)
        let mut indexed: FxHashSet<String> = FxHashSet::default();
        if let Some(extras) = call.arguments.get(2) {
            collect_indexed_columns_from_arg(extras, &mut indexed);
        }

        for (name, byte_offset) in fk_columns {
            if indexed.contains(&name) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, byte_offset as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "FK `.references()` without `.index()` \
                          — PostgreSQL does NOT auto-index FK columns. \
                          Add an explicit index to avoid sequential scans \
                          on JOINs and cascading deletes."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn property_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Returns true if `expr` contains a `.references(...)` call anywhere in its
/// receiver chain (not inside arguments), e.g. `uuid().notNull().references(...)`.
fn expr_calls_references(expr: &Expression) -> bool {
    let mut current = expr;
    loop {
        match current {
            Expression::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    if member.property.name.as_str() == "references" {
                        return true;
                    }
                    current = &member.object;
                } else {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

fn collect_indexed_columns_from_arg<'a>(arg: &Argument<'a>, out: &mut FxHashSet<String>) {
    let body: &FunctionBody<'a> = match arg {
        Argument::ArrowFunctionExpression(a) => &a.body,
        Argument::FunctionExpression(f) => match f.body.as_deref() {
            Some(b) => b,
            None => return,
        },
        _ => return,
    };

    // Arrow expression-body: `(t) => [<exprs>]` or `(t) => ({ ... })`
    // is desugared to a FunctionBody with a single ReturnStatement
    // by oxc. Block-body: `(t) => { return [...] }` works the same.
    for stmt in &body.statements {
        match stmt {
            Statement::ReturnStatement(ret) => {
                if let Some(expr) = ret.argument.as_ref() {
                    collect_from_returned_expression(expr, out);
                }
            }
            Statement::ExpressionStatement(es) => {
                collect_from_returned_expression(&es.expression, out);
            }
            _ => {}
        }
    }
}

fn collect_from_returned_expression<'a>(expr: &Expression<'a>, out: &mut FxHashSet<String>) {
    match expr {
        Expression::ArrayExpression(arr) => {
            for el in &arr.elements {
                if let Some(inner) = array_element_as_expression(el) {
                    collect_from_extras_entry(inner, out);
                }
            }
        }
        Expression::ObjectExpression(obj) => {
            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else {
                    continue;
                };
                collect_from_extras_entry(&p.value, out);
            }
        }
        // Parenthesized object literal: `(t) => ({ ... })`.
        Expression::ParenthesizedExpression(p) => {
            collect_from_returned_expression(&p.expression, out);
        }
        _ => {}
    }
}

fn array_element_as_expression<'b, 'a>(
    el: &'b ArrayExpressionElement<'a>,
) -> Option<&'b Expression<'a>> {
    match el {
        ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_) => None,
        other => other.as_expression(),
    }
}

fn collect_from_extras_entry<'a>(expr: &'a Expression<'a>, out: &mut FxHashSet<String>) {
    // Walk the call chain top-down (e.g. index("x").on(t.col).where(sql`...`))
    // looking for a `.on(...)` call whose receiver satisfies `is_index_chain`.
    // If found, collect args and return. Otherwise check for bare `primaryKey(...)`.
    let mut cursor: Option<&'a Expression<'a>> = Some(expr);
    while let Some(curr) = cursor {
        if let Expression::CallExpression(call) = curr {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                if member.property.name.as_str() == "on" && is_index_chain(&member.object) {
                    for arg in &call.arguments {
                        if let Some(name) = arg_as_member_property_name(arg) {
                            out.insert(name.to_string());
                        }
                    }
                    return;
                }
                // Peel one more layer of the chain.
                cursor = Some(&member.object);
                continue;
            }
            if let Expression::Identifier(id) = &call.callee {
                if id.name.as_str() == "primaryKey" {
                    if let Some(first) = leading_pk_column(call) {
                        out.insert(first.to_string());
                    }
                }
                return;
            }
        }
        cursor = None;
    }
}

/// Recognise the head of an `index(...)` / `uniqueIndex(...)` chain.
/// Accepts further chained calls (e.g. `.using(...)`) before `.on(...)`.
fn is_index_chain(expr: &Expression) -> bool {
    let mut current = expr;
    loop {
        match current {
            Expression::CallExpression(call) => match &call.callee {
                Expression::Identifier(id) => {
                    return matches!(id.name.as_str(), "index" | "uniqueIndex");
                }
                Expression::StaticMemberExpression(member) => {
                    current = &member.object;
                }
                _ => return false,
            },
            _ => return false,
        }
    }
}

fn arg_as_member_property_name<'b, 'a>(arg: &'b Argument<'a>) -> Option<&'b str> {
    let expr = arg.as_expression()?;
    if let Expression::StaticMemberExpression(member) = expr {
        return Some(member.property.name.as_str());
    }
    None
}

fn leading_pk_column<'b, 'a>(
    call: &'b oxc_ast::ast::CallExpression<'a>,
) -> Option<&'b str> {
    let first = call.arguments.first()?.as_expression()?;
    let Expression::ObjectExpression(obj) = first else {
        return None;
    };
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let Some(key) = property_key_name(&p.key) else {
            continue;
        };
        if key != "columns" {
            continue;
        }
        let Expression::ArrayExpression(arr) = &p.value else {
            return None;
        };
        let first = arr.elements.iter().find_map(array_element_as_expression)?;
        if let Expression::StaticMemberExpression(member) = first {
            return Some(member.property.name.as_str());
        }
        return None;
    }
    None
}
