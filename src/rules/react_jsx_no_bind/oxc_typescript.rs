//! react-jsx-no-bind OxcCheck backend. Files importing from `solid-js` are
//! exempt: SolidJS components do not re-render, so inline JSX functions never
//! cause extra renders and `useCallback` does not apply.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Module-level JSX is evaluated exactly once: there is no render cycle, so an
/// inline function cannot create per-render reference churn and `useCallback`
/// is not even usable there.
fn is_inside_function<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    semantic
        .nodes()
        .ancestors(node.id())
        .skip(1)
        .any(|a| matches!(a.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.source_contains("solid-js") {
            return;
        }
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        if !is_inside_function(node, semantic) {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };

            // Get the attribute name
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let attr_name = name_ident.name.as_str();

            // `ref` is not a diffed prop: React assigns it outside the
            // render/prop path and never re-renders on ref identity change, so
            // an inline ref callback (the standard array-of-refs pattern) is
            // not a churn concern.
            if attr_name == "ref" {
                continue;
            }

            // Value must be an expression container
            let Some(JSXAttributeValue::ExpressionContainer(ec)) = &attr.value else {
                continue;
            };

            let expr = match &ec.expression {
                JSXExpression::EmptyExpression(_) => continue,
                other => other,
            };

            let (kind_label, span) = match expr {
                JSXExpression::ArrowFunctionExpression(arrow) => {
                    ("arrow function", arrow.span)
                }
                JSXExpression::FunctionExpression(func) => {
                    ("function expression", func.span)
                }
                JSXExpression::CallExpression(call) => {
                    // Detect `foo.bind(...)`
                    let Expression::StaticMemberExpression(member) = &call.callee else {
                        continue;
                    };
                    if member.property.name.as_str() != "bind" {
                        continue;
                    }
                    ("`.bind()` call", call.span())
                }
                _ => continue,
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "{kind_label} as value of JSX prop `{attr_name}` creates a new reference every render \u{2014} hoist to `useCallback` or a stable handler."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_arrow_in_jsx_prop_react() {
        let src = "function App() { return <button onClick={() => f()} />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_arrow_in_jsx_prop_solid() {
        let src = "import { createSignal } from \"solid-js\";\nfunction App() { return <button onClick={() => f()} />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bind_in_jsx_prop_solid() {
        let src = "import { createSignal } from \"solid-js\";\nfunction App() { return <button onClick={this.f.bind(this)} />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_module_level_jsx_issue_1053() {
        // Regression for issue #1053: module-level JSX is evaluated once,
        // no render cycle, so inline functions are not a re-render hazard.
        let src = "const state = { description: <Trans bold={(text) => <strong>{text}</strong>} br={() => <br />} /> };";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }

    #[test]
    fn flags_jsx_inside_component_issue_1053() {
        let src = "function App() { return <button onClick={() => f()} />; }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_arrow_on_ref_attr_issue_1965() {
        // Regression for issue #1965: per-index ref callbacks in a `.map(...)`
        // are the standard array-of-refs pattern; `useCallback` cannot capture
        // a distinct index per element, and `ref` does not trigger re-renders.
        let src = "function App() { return views.map((view, index) => <HeaderControl ref={(node) => { r.current[index] = node; }} />); }";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }

    #[test]
    fn allows_bind_on_ref_attr_issue_1965() {
        let src = "function App() { return <div ref={this.setRef.bind(this)} />; }";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }

    #[test]
    fn flags_arrow_on_non_ref_attr_issue_1965() {
        let src = "function App() { return <button onClick={() => doThing()} />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bind_on_non_ref_attr_issue_1965() {
        let src = "function App() { return <button onClick={this.f.bind(this)} />; }";
        assert_eq!(run(src).len(), 1);
    }
}
