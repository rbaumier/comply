//! OXC backend for no-immediate-mutation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, SimpleAssignmentTarget, VariableDeclarationKind,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Mutating methods on arrays that indicate immediate mutation.
const ARRAY_MUTATORS: &[&str] = &[
    "push",
    "unshift",
    "pop",
    "shift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Walk all nodes looking for VariableDeclaration
        for node in semantic.nodes().iter() {
            let AstKind::VariableDeclaration(decl) = node.kind() else {
                continue;
            };

            // Only process declarations with exactly one declarator
            if decl.declarations.len() != 1 {
                continue;
            }

            let declarator = &decl.declarations[0];

            // Must have a simple identifier binding
            let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = declarator.id else {
                continue;
            };
            let var_name = id.name.as_str();

            // Must have an initializer
            let Some(ref init) = declarator.init else {
                continue;
            };

            // Determine what kind of literal
            let literal_kind = classify_init(init, ctx.source);
            if literal_kind == LiteralKind::None {
                continue;
            }

            // Find the next sibling statement by looking at the parent
            let parent_id = semantic.nodes().parent_id(node.id());
            if parent_id == node.id() {
                continue;
            }
            let parent = semantic.nodes().get_node(parent_id);

            // The parent should be something that contains statements
            // We need to find the next statement after this declaration
            let decl_end = decl.span.end;
            let Some(next_stmt_text) = find_next_statement_text(ctx.source, decl_end as usize) else {
                continue;
            };

            let next_stmt_text = next_stmt_text.trim();
            if next_stmt_text.is_empty() {
                continue;
            }

            let flagged = match literal_kind {
                LiteralKind::Array => {
                    is_method_call_on_text(next_stmt_text, var_name, ARRAY_MUTATORS)
                        || is_property_assignment_text(next_stmt_text, var_name)
                }
                LiteralKind::Object => {
                    is_property_assignment_text(next_stmt_text, var_name)
                }
                LiteralKind::Set => {
                    is_method_call_on_text(next_stmt_text, var_name, &["add"])
                }
                LiteralKind::Map => {
                    is_method_call_on_text(next_stmt_text, var_name, &["set"])
                }
                LiteralKind::None => false,
            };

            if flagged {
                // Report on the next statement position
                let next_offset = decl_end as usize + (ctx.source[decl_end as usize..].len() - next_stmt_text.len() - ctx.source[decl_end as usize..].trim_start().len() + next_stmt_text.len());
                // Simpler: find position of next_stmt in source after decl_end
                let after_decl = &ctx.source[decl_end as usize..];
                let trimmed = after_decl.trim_start();
                let offset = decl_end as usize + (after_decl.len() - trimmed.len());
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-immediate-mutation".into(),
                    message: "Immediate mutation after variable assignment \u{2014} chain onto the initialiser instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[derive(PartialEq)]
enum LiteralKind {
    None,
    Array,
    Object,
    Set,
    Map,
}

fn classify_init(expr: &Expression, _source: &str) -> LiteralKind {
    match expr {
        Expression::ArrayExpression(_) => LiteralKind::Array,
        Expression::ObjectExpression(_) => LiteralKind::Object,
        Expression::NewExpression(new_expr) => {
            if let Expression::Identifier(id) = &new_expr.callee {
                match id.name.as_str() {
                    "Set" | "WeakSet" => LiteralKind::Set,
                    "Map" | "WeakMap" => LiteralKind::Map,
                    _ => LiteralKind::None,
                }
            } else {
                LiteralKind::None
            }
        }
        _ => LiteralKind::None,
    }
}

/// Find the text of the next statement after a given byte offset.
fn find_next_statement_text(source: &str, after: usize) -> Option<&str> {
    let rest = source.get(after..)?;
    let trimmed = rest.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    // Find end of statement (next semicolon or newline-terminated expression)
    let end = trimmed.find(';').map(|i| i + 1)
        .or_else(|| trimmed.find('\n'))
        .unwrap_or(trimmed.len());
    Some(&trimmed[..end])
}

/// Check if text looks like `varName.method(...)` where method is in the list.
fn is_method_call_on_text(stmt: &str, var_name: &str, methods: &[&str]) -> bool {
    for method in methods {
        let pattern = format!("{var_name}.{method}(");
        if stmt.starts_with(&pattern) {
            return true;
        }
    }
    false
}

/// Check if text looks like `varName.prop = ...` or `varName[...] = ...`.
fn is_property_assignment_text(stmt: &str, var_name: &str) -> bool {
    if !stmt.starts_with(var_name) {
        return false;
    }
    let rest = &stmt[var_name.len()..];
    if rest.starts_with('.') || rest.starts_with('[') {
        // Must have an assignment somewhere
        return rest.contains('=') && !rest.starts_with("==");
    }
    false
}
