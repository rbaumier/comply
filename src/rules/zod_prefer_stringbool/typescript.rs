//! zod-prefer-stringbool backend — flag `z.coerce.boolean()` calls in files
//! that show form/HTML-input indicators. The smell is only meaningful for
//! string-typed inputs (form fields, query params); pure boolean coercion in
//! other contexts is fine.

use crate::diagnostic::{Diagnostic, Severity};

const FORM_INDICATORS: &[&str] = &[
    "react-hook-form",
    "@tanstack/react-form",
    "@tanstack/form",
    "formik",
    "FormData",
    "URLSearchParams",
    "useForm",
    "searchParams",
];

fn file_has_form_context(source: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(source) else {
        return false;
    };
    FORM_INDICATORS.iter().any(|m| text.contains(m))
}

crate::ast_check! { on ["call_expression"] prefilter = ["z.coerce.boolean"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Ok(func_text) = func.utf8_text(source) else { return };
    if func_text != "z.coerce.boolean" { return; }
    if !file_has_form_context(source) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`z.coerce.boolean()` treats every non-empty string as `true` — \
                  use `z.stringbool()` for HTML form inputs and query strings.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_coerce_boolean_with_useform() {
        let src = "import { useForm } from 'react-hook-form';\nconst S = z.coerce.boolean();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_coerce_boolean_with_searchparams() {
        let src = "const params = new URLSearchParams(); const S = z.coerce.boolean();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_stringbool() {
        let src = "import { useForm } from 'react-hook-form';\nconst S = z.stringbool();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_coerce_number() {
        let src = "import { useForm } from 'react-hook-form';\nconst S = z.coerce.number();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_coerce_boolean_outside_form_context() {
        // No form/HTML-input context → don't flag.
        assert!(run("const S = z.coerce.boolean();").is_empty());
    }
}
