//! ts-no-empty-function OxcCheck backend.
//!
//! Flag functions/methods with empty bodies that contain no comments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::FunctionBody;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Returns true when the function expression sits in a JSX expression container
/// or as an argument to a call/new expression (including parenthesized).
fn is_placeholder_callback_position(
    nodes: &oxc_semantic::AstNodes,
    node_id: oxc_semantic::NodeId,
) -> bool {
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return false;
    }
    match nodes.kind(parent_id) {
        AstKind::JSXExpressionContainer(_) => true,
        AstKind::CallExpression(call) => {
            let node_span = nodes.kind(node_id).span();
            call.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::NewExpression(new_expr) => {
            let node_span = nodes.kind(node_id).span();
            new_expr.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::ParenthesizedExpression(_) => {
            let grandparent_id = nodes.parent_id(parent_id);
            if grandparent_id == parent_id {
                return false;
            }
            matches!(
                nodes.kind(grandparent_id),
                AstKind::CallExpression(_)
                    | AstKind::NewExpression(_)
                    | AstKind::JSXExpressionContainer(_)
            )
        }
        _ => false,
    }
}

/// Returns true when the function body is empty (no statements, no directives)
/// and contains no comments in the source text between the braces.
fn is_empty_body(body: &FunctionBody, source: &str) -> bool {
    if !body.statements.is_empty() || !body.directives.is_empty() {
        return false;
    }
    // Check if there's a comment inside the body braces.
    let start = body.span.start as usize;
    let end = body.span.end as usize;
    if end > start && end <= source.len() {
        let inner = &source[start..end];
        // Strip outer braces
        let trimmed = inner.trim();
        if trimmed.len() > 2 {
            let content = trimmed[1..trimmed.len() - 1].trim();
            if content.starts_with("//") || content.starts_with("/*") {
                return false;
            }
        }
    }
    true
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
        let (body_opt, span, is_method) = match node.kind() {
            AstKind::Function(func) => {
                // Check if this is a constructor with parameter properties
                // by looking at parent for MethodDefinition context.
                let parent = semantic.nodes().parent_node(node.id());
                let is_method = matches!(parent.kind(), AstKind::MethodDefinition(_));
                (func.body.as_ref(), func.span, is_method)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                (Some(&arrow.body), arrow.span, false)
            }
            _ => return,
        };

        let Some(body) = body_opt else { return };

        // Arrow functions with expression bodies (no block) are never empty.
        if matches!(node.kind(), AstKind::ArrowFunctionExpression(arrow) if arrow.expression) {
            return;
        }

        if !is_empty_body(body, ctx.source) {
            return;
        }

        if is_test_file(ctx.path)
            && is_placeholder_callback_position(semantic.nodes(), node.id())
        {
            return;
        }

        // Skip constructors with parameter properties (accessibility modifiers).
        if is_method
            && let AstKind::MethodDefinition(method) = semantic.nodes().parent_node(node.id()).kind()
                && method.key.is_specific_id("constructor")
                    && let AstKind::Function(func) = node.kind() {
                        for param in &func.params.items {
                            if param.accessibility.is_some() {
                                return;
                            }
                        }
                    }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected empty function.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::{run_oxc_tsx, run_oxc_tsx_with_path};

    #[test]
    fn allows_empty_arrow_in_jsx_prop_in_test_file() {
        let src = r#"
            const x = <Foo onClose={() => {}} />;
        "#;
        assert!(run_oxc_tsx_with_path(src, &Check, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_empty_function_expression_in_jsx_prop_in_test_file() {
        let src = r#"
            const x = <Foo onClose={function () {}} />;
        "#;
        assert!(run_oxc_tsx_with_path(src, &Check, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_empty_arrow_as_call_argument_in_test_file() {
        let src = r#"
            useEffect(() => {}, []);
        "#;
        assert!(run_oxc_tsx_with_path(src, &Check, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_parenthesized_empty_arrow_as_call_argument_in_test_file() {
        // Regression: useEffect((() => {}), []) — ParenthesizedExpression parent
        // must not fall through to the `_ => false` arm.
        let src = r#"
            useEffect((() => {}), []);
        "#;
        assert!(run_oxc_tsx_with_path(src, &Check, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn flags_empty_arrow_in_variable_assignment_in_test_file() {
        // Negative control: direct assignment is not a placeholder callback position.
        let src = r#"
            const handler = () => {};
        "#;
        let diags = run_oxc_tsx_with_path(src, &Check, "Foo.test.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_named_function_declaration_in_test_file() {
        let src = r#"
            function doNothing() {}
        "#;
        let diags = run_oxc_tsx_with_path(src, &Check, "Foo.test.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_in_jsx_prop_in_non_test_file() {
        let src = r#"
            const x = <Foo onClose={() => {}} />;
        "#;
        let diags = run_oxc_tsx(src, &Check);
        assert_eq!(diags.len(), 1);
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_empty_function() {
        let diags = run_on("function foo() {}");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_empty_arrow_function() {
        let diags = run_on("const foo = () => {};");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_function_with_body() {
        assert!(run_on("function foo() { return 1; }").is_empty());
    }


    #[test]
    fn allows_function_with_comment() {
        assert!(run_on("function foo() { /* intentional */ }").is_empty());
    }
}
