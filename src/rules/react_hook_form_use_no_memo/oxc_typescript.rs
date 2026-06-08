//! react-hook-form-use-no-memo oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::Semantic;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useForm"])
    }

    fn run_on_semantic<'a>(&self, semantic: &'a Semantic<'a>, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // A `"use no memo"` directive anywhere — file-level or inside the
        // component body — satisfies the convention.
        let has_use_no_memo = semantic.nodes().iter().any(|n| {
            let directives = match n.kind() {
                AstKind::Program(p) => &p.directives,
                AstKind::Function(f) => match f.body.as_ref() {
                    Some(b) => &b.directives,
                    None => return false,
                },
                AstKind::ArrowFunctionExpression(a) => &a.body.directives,
                _ => return false,
            };
            directives.iter().any(|d| d.expression.value == "use no memo")
        });

        if has_use_no_memo {
            return Vec::new();
        }

        // Find the first bare `useForm(...)` call. Renamed/member forms
        // (`useFormContext`, `methods.useForm`) are intentionally not matched.
        let useform_span = semantic.nodes().iter().find_map(|n| {
            let AstKind::CallExpression(call) = n.kind() else { return None };
            let Expression::Identifier(id) = &call.callee else { return None };
            (id.name.as_str() == "useForm").then_some(call.span.start)
        });

        let Some(span_start) = useform_span else { return Vec::new() };

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "This file calls `useForm` but has no `\"use no memo\"` directive. The \
                      React Compiler memoizes the form proxy incorrectly \u{2014} add \
                      `\"use no memo\"` to opt this file out."
                .into(),
            severity: Severity::Warning,
            span: None,
        }]
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
    fn flags_useform_without_directive() {
        let src = r#"
            export function EditForm() {
              const form = useForm();
              return <form />;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_useform_with_file_directive() {
        let src = r#"
            "use no memo";
            export function EditForm() {
              const form = useForm();
              return <form />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_useform_with_body_directive() {
        let src = r#"
            export function EditForm() {
              "use no memo";
              const form = useForm();
              return <form />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_use_form_context() {
        // `useFormContext` consumes an existing form; it needs no opt-out.
        let src = r#"
            export function Field() {
              const { register } = useFormContext();
              return <input {...register("x")} />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_file_without_useform() {
        let src = r#"export function Plain() { return <div />; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn regression_amadeo_use_form() {
        // amadeo runs the React Compiler; every `useForm` file carries
        // `"use no memo"`. A file missing it is a defect.
        let src = r#"
            export function CreateClientDialog() {
              const form = useForm({ resolver: zodResolver(schema) });
              return <form onSubmit={form.handleSubmit(onSubmit)} />;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
