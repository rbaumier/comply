//! OXC backend for react-no-boolean-variant-props.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use std::sync::Arc;

pub struct Check;

fn looks_like_variant_prop(name: &str) -> bool {
    let check = |prefix: &str| -> bool {
        if !name.starts_with(prefix) {
            return false;
        }
        let rest = &name[prefix.len()..];
        rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())
    };
    check("is") || check("has")
}

fn function_name_is_component(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn count_boolean_variants(pattern: &oxc_ast::ast::ObjectPattern) -> usize {
    let mut count = 0usize;
    for prop in &pattern.properties {
        let name_str: Option<String> = if prop.shorthand {
            match &prop.value {
                BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
                BindingPattern::AssignmentPattern(assign) => match &assign.left {
                    BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
                    _ => None,
                },
                _ => None,
            }
        } else {
            prop.key.static_name().map(|s| s.to_string())
        };
        if let Some(ref n) = name_str {
            if looks_like_variant_prop(n) {
                count += 1;
            }
        }
    }
    count
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
        let is_component = match node.kind() {
            AstKind::Function(func) => {
                func.id
                    .as_ref()
                    .is_some_and(|id| function_name_is_component(id.name.as_str()))
            }
            AstKind::ArrowFunctionExpression(_) => {
                let parent = semantic.nodes().parent_node(node.id());
                let AstKind::VariableDeclarator(decl) = parent.kind() else {
                    return;
                };
                match &decl.id {
                    BindingPattern::BindingIdentifier(id) => {
                        function_name_is_component(id.name.as_str())
                    }
                    _ => false,
                }
            }
            _ => return,
        };
        if !is_component {
            return;
        }

        let params = match node.kind() {
            AstKind::Function(func) => &func.params,
            AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
            _ => return,
        };
        let Some(first_param) = params.items.first() else {
            return;
        };

        let object_pattern = match &first_param.pattern {
            BindingPattern::ObjectPattern(pat) => Some(pat.as_ref()),
            BindingPattern::AssignmentPattern(assign) => match &assign.left {
                BindingPattern::ObjectPattern(pat) => Some(pat.as_ref()),
                _ => None,
            },
            _ => None,
        };
        let Some(pattern) = object_pattern else {
            return;
        };

        let count = count_boolean_variants(pattern);
        if count < 2 {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, pattern.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{count} boolean variant props on this component — collapse into a single \
                 `variant: '...' | '...'` union to eliminate mutually-exclusive invalid states."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
