//! react-no-namespace oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // SolidJS uses namespaced JSX (`use:`, `prop:`, `attr:`, `on:`, `bool:`)
        // as first-class directive syntax, valid in Solid but not React. Skip
        // SolidJS files so this React-only rule does not fire on them.
        if crate::oxc_helpers::imports_solid(ctx.source) {
            return;
        }

        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        // Check element name for namespace.
        if let JSXElementName::NamespacedName(ns) = &opening.name {
            let name = format!("{}:{}", ns.namespace.name, ns.name.name);
            let (line, column) =
                byte_offset_to_line_col(ctx.source, ns.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Namespaced JSX element `{name}` is not supported by React."
                ),
                severity: Severity::Error,
                span: None,
            });
        }

        // Check attributes for namespace.
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            if let JSXAttributeName::NamespacedName(ns) = &attr.name {
                let name = format!("{}:{}", ns.namespace.name, ns.name.name);
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ns.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Namespaced JSX attribute `{name}` is not supported by React."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_namespaced_element() {
        let src = "const x = <ns:div />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_namespaced_attribute() {
        let src = r#"const x = <div ns:attr="val" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_normal_element() {
        let src = "const x = <div className=\"a\" />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_solid_use_directive() {
        let src = r#"
import { createSignal } from "solid-js";
const x = <input use:setFocus />;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_solid_prop_namespace() {
        let src = r#"
import { createSignal } from "solid-js";
const x = <div prop:value={x} />;
"#;
        assert!(run(src).is_empty());
    }
}
