//! tailwind-enforce-logical-properties oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::path_utils::is_in_framework_entry_dir;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["className", "class"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Email templates (react-email convention dirs like `emails/`) must use
        // physical directional classes: HTML email clients (Outlook, Gmail, Apple
        // Mail) don't support CSS logical properties, so `ps-`/`pe-` would break
        // the layout. The dir check only fires when react-email is detected, so a
        // same-named directory in a non-email project stays flaggable.
        if is_in_framework_entry_dir(ctx.path, ctx.project) {
            return;
        }
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        let name = ident.name.as_str();
        if name != "className" && name != "class" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let Some(logical_hint) = super::has_physical_directional_spacing(lit.value.as_str()) else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Physical directional spacing — prefer the logical equivalent \
                 (e.g. `{logical_hint}…`) so the layout flips correctly in RTL."
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

    #[test]
    fn flags_ml_class() {
        let src = r#"const x = <div className="ml-4" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_pr_class() {
        let src = r#"const x = <div className="pr-2 mb-4" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_logical_classes() {
        let src = r#"const x = <div className="ms-4 me-2 ps-1 pe-1" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_neutral_spacing() {
        let src = r#"const x = <div className="m-4 p-2 mt-1 mb-2" />;"#;
        assert!(run(src).is_empty());
    }

    /// #1355: email templates require physical directional classes because email
    /// clients don't support CSS logical properties. A `pr-2.5` in a react-email
    /// `emails/` file must not be flagged.
    #[test]
    fn ignores_physical_classes_in_react_email_template() {
        let src = r#"const x = <div className="pr-2.5 pl-1" />;"#;
        let project = crate::project::ProjectCtx::for_test_with_framework("react-email");
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let out = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            "apps/demo/emails/01-Barebone/product-update.tsx",
            &project,
            file,
        );
        assert!(out.is_empty(), "react-email template must not be flagged");
    }

    /// Negative space: the same classes in a non-email file of a react-email
    /// project are still flagged — the exemption is scoped to email convention
    /// dirs, not the whole project.
    #[test]
    fn flags_physical_classes_outside_email_dir_in_react_email_project() {
        let src = r#"const x = <div className="pr-2.5 pl-1" />;"#;
        let project = crate::project::ProjectCtx::for_test_with_framework("react-email");
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let out = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            "apps/demo/src/components/button.tsx",
            &project,
            file,
        );
        assert_eq!(out.len(), 1, "non-email file must still be flagged");
    }

    /// Negative space: an `emails/` path without react-email detected is still
    /// flagged — the directory name alone never exempts a non-email project.
    #[test]
    fn flags_physical_classes_in_emails_dir_without_react_email() {
        let src = r#"const x = <div className="pr-2.5 pl-1" />;"#;
        let out =
            crate::rules::test_helpers::run_rule(&Check, src, "src/emails/welcome.tsx");
        assert_eq!(
            out.len(),
            1,
            "emails/ dir without react-email must still be flagged"
        );
    }
}
