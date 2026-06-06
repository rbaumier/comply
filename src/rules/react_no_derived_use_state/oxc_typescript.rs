//! OxcCheck backend for react-no-derived-use-state.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, Expression};
use std::sync::Arc;

pub struct Check;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn is_use_state_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name.as_str() == "useState",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "React" && member.property.name.as_str() == "useState"
            } else {
                false
            }
        }
        _ => false,
    }
}

fn collect_destructured_prop_names<'a>(
    pattern: &'a BindingPattern<'a>,
) -> Vec<&'a str> {
    let mut out = Vec::new();
    if let BindingPattern::ObjectPattern(obj) = pattern {
        for prop in &obj.properties {
            match &prop.value {
                BindingPattern::BindingIdentifier(id) => {
                    out.push(id.name.as_str());
                }
                BindingPattern::AssignmentPattern(assign) => {
                    if let BindingPattern::BindingIdentifier(id) = &assign.left {
                        out.push(id.name.as_str());
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn extract_prop_names_from_params<'a>(
    params: &'a oxc_ast::ast::FormalParameters<'a>,
) -> Vec<&'a str> {
    let Some(first) = params.items.first() else {
        return vec![];
    };
    collect_destructured_prop_names(&first.pattern)
}

/// Walk ancestors to find the enclosing component and extract prop names.
fn find_component_prop_names<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Vec<&'a str> {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()).skip(1) {
        match ancestor.kind() {
            AstKind::Function(func) => {
                let name = func.id.as_ref().map(|id| id.name.as_str()).unwrap_or("");
                if !starts_with_uppercase(name) {
                    continue;
                }
                return extract_prop_names_from_params(&func.params);
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                let is_component = nodes
                    .ancestors(ancestor.id())
                    .nth(1)
                    .is_some_and(|p| {
                        if let AstKind::VariableDeclarator(decl) = p.kind()
                            && let BindingPattern::BindingIdentifier(id) = &decl.id {
                                return starts_with_uppercase(id.name.as_str());
                            }
                        false
                    });
                if !is_component {
                    continue;
                }
                return extract_prop_names_from_params(&arrow.params);
            }
            AstKind::Program(_) => return vec![],
            _ => continue,
        }
    }
    vec![]
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useState"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_use_state_call(call) {
            return;
        }
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Argument::Identifier(arg_ident) = first_arg else {
            return;
        };
        let arg_name = arg_ident.name.as_str();

        // `default*` props are initial-value props (controlled/uncontrolled pattern) — not derived state.
        if arg_name.starts_with("default") {
            return;
        }

        let prop_names = find_component_prop_names(node, semantic);
        if !prop_names.contains(&arg_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`useState` initialized from prop `{arg_name}` — derive during render or use `key` prop to reset."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
