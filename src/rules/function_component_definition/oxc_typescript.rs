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

/// True when the program imports from `solid-js` or any `solid-js/*` subpath
/// (`solid-js/web`, `solid-js/store`, …). Solid renders JSX but is not React:
/// its components are plain arrow functions by convention, so the React-only
/// "use a function declaration" guidance does not apply to a Solid file.
fn imports_solid_js(semantic: &oxc_semantic::Semantic) -> bool {
    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return false;
        };
        let source = import.source.value.as_str();
        source == "solid-js" || source.starts_with("solid-js/")
    })
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

        // Solid.js files use JSX but are not React; arrow-function components
        // are idiomatic there, so the rule does not apply (issue #1924).
        if imports_solid_js(semantic) {
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_react_arrow_component() {
        let src = "export const Display = (props) => <div>{props.x}</div>;";
        assert_eq!(run(src).len(), 1);
    }

    // Regression test for #1924: a Solid.js arrow-function component that
    // imports from `solid-js/web` must not be flagged — Solid is not React.
    #[test]
    fn allows_solid_arrow_component() {
        let src = r#"import { createStore, useSelector } from '@tanstack/solid-store'
import { render } from 'solid-js/web'

export const Display = (props) => {
  const count = useSelector(store, (state) => state[props.animals])
  return <div>{props.animals}: {count()}</div>
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_solid_arrow_component_bare_import() {
        let src = r#"import { createSignal } from 'solid-js'

export const Counter = () => {
  const [count] = createSignal(0)
  return <div>{count()}</div>
}
"#;
        assert!(run(src).is_empty());
    }
}
