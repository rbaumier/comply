//! OxcCheck backend for react-no-object-type-as-default-prop.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, FormalParameters};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Check whether a binding pattern contains `= {}`, `= []`, or `= () => ...` defaults.
fn check_default_value_in_binding(pattern: &BindingPattern) -> bool {
    match pattern {
        BindingPattern::AssignmentPattern(assign) => {
            matches!(
                &assign.right,
                Expression::ObjectExpression(_)
                    | Expression::ArrayExpression(_)
                    | Expression::ArrowFunctionExpression(_)
            )
        }
        BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                if check_default_value_in_binding(&prop.value) {
                    return true;
                }
            }
            false
        }
        BindingPattern::ArrayPattern(arr) => {
            for elem in arr.elements.iter().flatten() {
                if check_default_value_in_binding(elem) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

fn find_object_pattern_with_defaults(params: &FormalParameters) -> Option<oxc_span::Span> {
    let first = params.items.first()?;
    check_pattern_for_object_defaults(&first.pattern)
}

fn check_pattern_for_object_defaults(pattern: &BindingPattern) -> Option<oxc_span::Span> {
    match pattern {
        BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                if check_default_value_in_binding(&prop.value) {
                    return Some(obj.span);
                }
            }
            None
        }
        BindingPattern::AssignmentPattern(assign) => {
            check_pattern_for_object_defaults(&assign.left)
        }
        _ => None,
    }
}

fn check_function_params(
    name: &str,
    params: &FormalParameters,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !starts_with_uppercase(name) {
        return;
    }
    if let Some(span) = find_object_pattern_with_defaults(params) {
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Object/array/function default prop creates a new \
                      reference every render, breaking `React.memo`. Move \
                      the default to a module-level constant."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Function(func) => {
                if let Some(id) = &func.id {
                    check_function_params(id.name.as_str(), &func.params, ctx, diagnostics);
                }
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                let nodes = semantic.nodes();
                if let Some(parent) = nodes.ancestors(node.id()).nth(1) {
                    if let AstKind::VariableDeclarator(decl) = parent.kind() {
                        if let BindingPattern::BindingIdentifier(id) = &decl.id {
                            check_function_params(
                                id.name.as_str(),
                                &arrow.params,
                                ctx,
                                diagnostics,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
