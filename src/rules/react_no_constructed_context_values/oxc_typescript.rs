//! react-no-constructed-context-values OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName, JSXExpression,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Provider"])
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

        // Skip non-React JSX (SolidJS, Vue, Preact, Qwik…). The re-render concern
        // is React-only: those frameworks are signal-based, so an inline object/array
        // `value` does not re-construct on every render, and the `useMemo` remedy
        // does not exist there. Mirrors `react-style-prop-object`'s exemption.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        // Tag must contain "Provider".
        let tag_str = match &opening.name {
            JSXElementName::Identifier(id) => id.name.as_str().to_string(),
            JSXElementName::MemberExpression(member) => {
                format!("{}.{}", member.object, member.property.name)
            }
            _ => return,
        };
        if !tag_str.contains("Provider") {
            return;
        }

        // Find the `value` attribute.
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            if name_ident.name.as_str() != "value" {
                continue;
            }

            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };

            let is_inline = match &container.expression {
                JSXExpression::ObjectExpression(_) | JSXExpression::ArrayExpression(_) => true,
                _ => false,
            };

            if is_inline {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Context Provider `value` is an inline object/array — \
                              a new reference is created every render, causing all \
                              consumers to re-render. Memoize with `useMemo`."
                        .into(),
                    severity: Severity::Warning,
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
    fn flags_inline_object_in_react_file() {
        // A genuine React file (imports `react`, no Solid markers) must still be
        // flagged: the inline object reconstructs on every render.
        let src = "import { createContext } from 'react';\nconst x = <MyContext.Provider value={{ a, b }}>child</MyContext.Provider>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inline_array_in_solid_js_file() {
        // The issue's FP: a `.js` file importing `solid-js`. SolidJS is
        // signal-based — the re-render concern (and `useMemo` remedy) does not
        // apply. (Closes #3285)
        let src = "import { createContext } from 'solid-js';\nconst x = <RouterContext.Provider value={[location, pending]}>child</RouterContext.Provider>;";
        assert!(run(src).is_empty());
    }
}
