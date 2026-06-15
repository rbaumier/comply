//! react-style-prop-object oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue};
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
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if name_ident.name.as_str() != "style" {
                continue;
            }

            // Skip non-React JSX (SolidJS, Vue, Preact, Qwik…). Those frameworks
            // accept `style="css-string"` as valid JSX; the object-syntax advice
            // is React-only. Mirrors `no-unknown-property`'s exemption.
            if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
                return;
            }

            // Flag if value is a string literal (not an expression container).
            if let Some(JSXAttributeValue::StringLiteral(_)) = &attr.value {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "The `style` prop expects a JavaScript object, \
                              not a CSS string. Use `style={{ ... }}` instead."
                        .into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_string_style_in_react_jsx() {
        let src = r#"const x = <div style="color:red">hello</div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_string_style_in_react_file_importing_react() {
        // Genuine React file (imports `react`, no Solid markers) must still be
        // flagged with the object-syntax suggestion.
        let src = "import { useState } from 'react';\nconst x = <div style=\"color:red\">hi</div>;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("style={{"));
    }

    #[test]
    fn allows_object_style() {
        let src = r#"const x = <div style={{ color: "red" }}>hello</div>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_string_style_in_solid_jsx() {
        // SolidJS accepts `style="css-string"` as valid JSX. A file importing
        // `solid-js` must not be flagged. (Closes #2216)
        let src =
            "import { ErrorBoundary } from 'solid-js';\nconst x = <span style=\"font-size:1.5em\">{message}</span>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_string_style_in_solid_type_only_import() {
        // The icons.tsx example: a type-only import from solid-js.
        let src = "import type { JSX } from 'solid-js';\nconst x = <svg style=\"width:24px;height:24px\" />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_string_style_with_non_react_jsx_import_source_pragma() {
        // Hono's `src/jsx/index.test.tsx`: a `@jsxImportSource` pragma points at a
        // relative non-React runtime (Hono's own JSX), which renders a string-valued
        // `style` as-is. The object-syntax advice is React-only. (Closes #3220)
        let src = "/** @jsxImportSource ./ */\n\
                   const template = <h1 style='color:red;font-size:small'>Hello</h1>;";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn flags_string_style_with_react_jsx_import_source_pragma() {
        // A `@jsxImportSource react` pragma still names React — the object-syntax
        // suggestion stays. The exemption requires a *non-React* source.
        let src = "/** @jsxImportSource react */\n\
                   const x = <div style=\"color:red\">hi</div>;";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        assert!(diags[0].message.contains("style={{"));
    }
}
