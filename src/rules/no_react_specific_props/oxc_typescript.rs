//! no-react-specific-props oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// React-specific JSX prop names paired with their DOM-native replacement.
const REACT_SPECIFIC_PROPS: &[(&str, &str)] = &[("className", "class"), ("htmlFor", "for")];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["className", "htmlFor"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // React-only files use `className`/`htmlFor` correctly. Fire only in
        // non-React JSX (Solid, Qwik, Vue JSX, Preact, Stencil), where the
        // DOM-native attribute names are the supported form.
        if !crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        let Some((react_prop, native_prop)) = REACT_SPECIFIC_PROPS
            .iter()
            .find(|(name, _)| *name == ident.name.as_str())
        else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{react_prop}` is a React-specific prop not supported by non-React \
                 frameworks. Use `{native_prop}` instead."
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

    // --- Invalid (Biome fixtures), gated to non-React JSX via a solid-js import ---

    #[test]
    fn flags_class_name_in_solid_jsx() {
        // Biome invalid fixture: <Hello className="John" />, in a Solid file.
        let src = "import { createSignal } from 'solid-js';\n\
                   const x = <Hello className=\"John\" />;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("class"));
    }

    #[test]
    fn flags_html_for_in_solid_jsx() {
        let src = "import { createSignal } from 'solid-js';\n\
                   const x = <label htmlFor=\"id\">Name</label>;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`for`"));
    }

    // --- Valid (Biome fixtures): native attribute names ---

    #[test]
    fn allows_class_in_solid_jsx() {
        // Biome valid fixture: <Hello class="Doe" />.
        let src = "import { createSignal } from 'solid-js';\n\
                   const x = <Hello class=\"Doe\" />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_in_solid_jsx() {
        let src = "import { createSignal } from 'solid-js';\n\
                   const x = <label for=\"id\">Name</label>;";
        assert!(run(src).is_empty());
    }

    // --- Over-firing guard: must NOT fire in a React project ---

    #[test]
    fn allows_class_name_in_react_file() {
        // A genuine React file (imports `react`, no non-React JSX markers).
        // `className` is correct here and must not be flagged.
        let src = "import { useState } from 'react';\n\
                   const x = <Hello className=\"John\" />;";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn allows_class_name_in_plain_jsx_file() {
        // No framework markers at all: default to React intent, do not fire.
        let src = "const x = <Hello className=\"John\" htmlFor=\"id\" />;";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn flags_both_props_in_solid_jsx() {
        let src = "import { createSignal } from 'solid-js';\n\
                   const x = <label className=\"a\" htmlFor=\"b\">x</label>;";
        assert_eq!(run(src).len(), 2);
    }
}
