//! function-component-definition OXC backend — flag React arrow-function components.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_test_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("/tests/")
        || s.contains("\\tests\\")
}

fn starts_with_uppercase(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// Check if any node under `start` contains JSX by iterating all nodes
/// whose byte range falls within the start node's span.
fn contains_jsx(start: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let start_span = match start.kind() {
        AstKind::VariableDeclarator(d) => d.span,
        _ => return false,
    };
    for node in semantic.nodes().iter() {
        if let AstKind::JSXOpeningElement(el) = node.kind()
            && el.span.start >= start_span.start && el.span.end <= start_span.end {
                return true;
            }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return;
        }

        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };

        let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &decl.id else {
            return;
        };
        let name = binding.name.as_str();
        if !starts_with_uppercase(name) {
            return;
        }

        let Some(Expression::ArrowFunctionExpression(_arrow)) = &decl.init else {
            return;
        };

        // Check if the arrow function body contains JSX.
        if !contains_jsx(node, semantic) {
            return;
        }

        let span = match &decl.init {
            Some(expr) => oxc_span::GetSpan::span(expr),
            None => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "React component `{name}` should be a `function` declaration, not an arrow function."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_arrow_component_self_closing() {
        let src = "const MyComponent = () => <div />;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_arrow_component_with_block_body() {
        let src = "const MyComponent = (props) => { return <div>{props.x}</div>; };";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_exported_arrow_component() {
        let src = "export const MyComponent = () => <div />;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_function_declaration_component() {
        let src = "function MyComponent() { return <div />; }";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_non_pascal_arrow() {
        let src = "const handler = () => <div />;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_arrow_without_jsx() {
        let src = "const myUtil = () => someValue;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_pascal_arrow_without_jsx() {
        let src = "const MyThing = () => someValue;";
        assert!(run_on(src).is_empty());
    }
}
