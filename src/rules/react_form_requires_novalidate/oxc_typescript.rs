//! react-form-requires-novalidate oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["<form"])
    }

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

        // Only the native lowercase `<form>`. A PascalCase `<Form>` is a
        // component wrapper that owns its own validation contract.
        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        if tag_ident.name.as_str() != "form" {
            return;
        }

        let mut has_novalidate = false;
        let mut has_spread = false;
        for attr_item in &opening.attributes {
            match attr_item {
                // `<form {...props}>` could carry noValidate through the spread;
                // we can't prove its absence, so don't flag.
                JSXAttributeItem::SpreadAttribute(_) => has_spread = true,
                JSXAttributeItem::Attribute(attr) => {
                    if let JSXAttributeName::Identifier(name_ident) = &attr.name
                        && name_ident.name.as_str().eq_ignore_ascii_case("novalidate")
                    {
                        has_novalidate = true;
                    }
                }
            }
        }

        if has_novalidate || has_spread {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Native `<form>` is missing `noValidate` \u{2014} the browser will run its \
                      own HTML validation alongside the app's. Add `noValidate`."
                .into(),
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

    #[test]
    fn flags_form_without_novalidate() {
        let src = r#"const x = <form onSubmit={handle}>body</form>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_form_with_novalidate() {
        let src = r#"const x = <form noValidate onSubmit={handle}>body</form>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_form_with_novalidate_explicit_true() {
        let src = r#"const x = <form noValidate={true}>body</form>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_form_component_wrapper() {
        // PascalCase <Form> is a component, not the native element.
        let src = r#"const x = <Form onSubmit={handle}>body</Form>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_form_with_spread() {
        // Spread could carry noValidate; absence is unprovable.
        let src = r#"const x = <form {...formProps}>body</form>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn regression_amadeo_form_dialog() {
        // MR !392 / #409 — every native `<form>` must carry noValidate so the
        // RHF + Zod layer owns validation instead of the browser.
        let src = r#"
            function EditDialog() {
              return (
                <form onSubmit={form.handleSubmit(onSubmit)}>
                  <input {...form.register("name")} />
                </form>
              );
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_form_in_test_file_issue1347() {
        // Issue #1347: forms rendered in a test runner have no browser UI, so
        // native validation never fires and `noValidate` is meaningless.
        // `skip_in_test_dir` suppresses the rule for files the central
        // predicate classifies as tests (here, `__tests__/` + `.test.`).
        let src = r#"const x = <form onSubmit={handle}>body</form>;"#;
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "src/__tests__/useFieldArray.test.tsx",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn still_flags_form_in_production_file_issue1347() {
        // Negative space for #1347: the same `<form>` in a production file is
        // still flagged — the gate only exempts test files.
        let src = r#"const x = <form onSubmit={handle}>body</form>;"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/EditForm.tsx");
        assert_eq!(d.len(), 1);
    }
}
