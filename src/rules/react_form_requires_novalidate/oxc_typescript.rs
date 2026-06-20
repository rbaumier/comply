//! react-form-requires-novalidate oxc backend.
//!
//! Flags a native lowercase `<form>` that lacks a `noValidate` attribute.
//! Non-React JSX files (Vue `defineComponent`/TSX, Solid, Preact, …) are exempt:
//! the dual-validation hazard is React-specific to its controlled inputs.
//! Storybook story files (`*.stories.tsx`) are exempt: they are component demos
//! rendered in an isolated iframe where native HTML validation is expected, so
//! `noValidate` would suppress the behavior the story demonstrates.

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
        // The dual-validation hazard this rule targets is React-specific: React's
        // controlled inputs bypass native HTML validation. A non-React JSX file
        // (Vue `defineComponent`/TSX, Solid, Preact, …) has its own validation
        // lifecycle and no such conflict, so `<form>` there must not be flagged.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        // Storybook story files (`*.stories.tsx`) are component demos rendered in
        // an isolated iframe where native HTML validation is expected (often the
        // behavior being demonstrated), not production app routes — same
        // non-production category as the test files already skipped. `noValidate`
        // would suppress the very validation the story shows.
        if crate::rules::path_utils::is_storybook_story(ctx.path) {
            return;
        }

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

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
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

    #[test]
    fn skips_form_in_storybook_story_issue4448() {
        // Issue #4448 (tremorlabs/tremor): a story renders a native `<form>` to
        // demo the component; Storybook runs it in an isolated iframe where
        // native HTML validation is expected (often the behavior shown), so
        // `noValidate` would suppress the very thing the story demonstrates.
        let src = r#"const x = <form className="flex">body</form>;"#;
        assert!(run_at(src, "src/components/RadioGroup/radiogroup.stories.tsx").is_empty());
        assert!(run_at(src, "src/components/RadioGroup/radiogroup.stories.ts").is_empty());
    }

    #[test]
    fn still_flags_form_in_regular_tsx_issue4448() {
        // Negative space for #4448: the exemption is path-scoped to story
        // files — the same `<form>` in a regular `.tsx` is still flagged.
        let src = r#"const x = <form className="flex">body</form>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_form_in_vue_tsx_define_component_issue5015() {
        // Issue #5015 (Tencent/tdesign-vue-next): a Vue 3 `defineComponent`
        // renders a native `<form>` in its `setup()` render function. The
        // dual-validation hazard is React-specific (controlled inputs); Vue runs
        // its own validation lifecycle, so the `vue` import marks this non-React
        // JSX and the rule must not fire.
        let src = r#"
            import { defineComponent, ref } from 'vue';
            export default defineComponent({
              setup(props) {
                const formRef = ref();
                return () => (
                  <form id={props.id} ref={formRef} onSubmit={(e) => onSubmit(e)}>
                    {renderContent('default')}
                  </form>
                );
              },
            });
        "#;
        assert!(run_at(src, "packages/components/form/form.tsx").is_empty());
    }

    #[test]
    fn still_flags_form_in_react_tsx_issue5015() {
        // Negative space for #5015: a genuine React `.tsx` `<form>` (React import,
        // no Vue markers) is still flagged.
        let src = r#"
            import React from 'react';
            function EditForm() {
              return <form onSubmit={handle}>body</form>;
            }
        "#;
        assert_eq!(run_at(src, "src/EditForm.tsx").len(), 1);
    }
}
