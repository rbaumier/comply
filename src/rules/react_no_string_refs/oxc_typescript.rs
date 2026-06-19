//! react-no-string-refs oxc backend.
//!
//! Flags `ref="stringValue"` on JSX elements (string refs are a deprecated React
//! API). Files for a non-React JSX framework (Vue, Solid, Preact, Qwik, Stencil)
//! are exempt: there `ref="name"` is the framework's own template-ref binding
//! (Vue exposes it as `this.$refs.name`), not React's legacy string ref.

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
        // String refs are a React-only concern. A Vue / Solid / Preact JSX file
        // uses `ref="x"` as its own template-ref API, so it must not be judged
        // by them.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

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
            if name_ident.name.as_str() != "ref" {
                continue;
            }
            // Check if the value is a string literal.
            if let Some(JSXAttributeValue::StringLiteral(_)) = &attr.value {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "String refs are deprecated — use `useRef()` or a \
                              callback ref instead."
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
    fn flags_string_ref_in_react() {
        let src = r#"const x = <input ref="myInput" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_string_ref_in_vue_tsx() {
        // Regression for issue #4523: `ref="x"` is Vue's template-ref API.
        let src = "import { defineComponent } from 'vue';\n\
                   const C = defineComponent({ render() { return <NxScrollbar ref=\"scrollbarInstRef\" />; } });";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }
}
