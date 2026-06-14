//! react-hook-form-validation-mode oxc backend — flag bare `useForm(...)` calls
//! that don't set `mode: "onTouched"` and `reValidateMode: "onChange"`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn prop_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

fn has_spread(obj: &ObjectExpression) -> bool {
    obj.properties
        .iter()
        .any(|p| matches!(p, ObjectPropertyKind::SpreadProperty(_)))
}

fn find_prop_value<'a, 'b>(obj: &'b ObjectExpression<'a>, needle: &str) -> Option<&'b Expression<'a>> {
    for p in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = p else { continue };
        if prop_key_name(&prop.key) == Some(needle) {
            return Some(&prop.value);
        }
    }
    None
}

/// Returns the remediation snippet for `key`/`expected` if the property is
/// missing or set to a string literal other than `expected`. A non-string-literal
/// value (a const, expression, etc.) is trusted and never flagged.
fn issue(obj: Option<&ObjectExpression>, key: &str, expected: &str) -> Option<String> {
    match obj.and_then(|o| find_prop_value(o, key)) {
        None => Some(format!("`{key}: \"{expected}\"`")),
        Some(Expression::StringLiteral(s)) if s.value.as_str() != expected => {
            Some(format!("`{key}: \"{expected}\"`"))
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useForm"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Only bare `useForm(...)`. Renamed/member forms (`useFormContext`,
        // `methods.useForm`) are intentionally not matched.
        let Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "useForm" {
            return;
        }

        // `mode`/`reValidateMode` are React-Hook-Form-specific options. A
        // `useForm` imported from another library (e.g. `@tanstack/react-form`)
        // has a different API, so this rule must not fire on it.
        if crate::oxc_helpers::local_binding_imported_from_foreign_package(semantic, "useForm") {
            return;
        }

        // No arg → both options missing. An object literal is inspected directly.
        // Any other config (a variable, a spread) can't be verified statically.
        let obj = match call.arguments.first() {
            None => None,
            Some(arg) => match arg.as_expression() {
                Some(Expression::ObjectExpression(o)) if has_spread(o) => return,
                Some(Expression::ObjectExpression(o)) => Some(&**o),
                _ => return,
            },
        };

        let missing: Vec<String> = [("mode", "onTouched"), ("reValidateMode", "onChange")]
            .into_iter()
            .filter_map(|(key, expected)| issue(obj, key, expected))
            .collect();

        if missing.is_empty() {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`useForm` must set {}.", missing.join(" and ")),
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
    fn flags_missing_both_options() {
        let src = r#"
            export function EditForm() {
              const form = useForm({ resolver: zodResolver(schema) });
              return <form />;
            }
        "#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("onTouched"));
        assert!(d[0].message.contains("onChange"));
    }

    #[test]
    fn flags_no_arg_useform() {
        let src = r#"
            export function EditForm() {
              const form = useForm();
              return <form />;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_wrong_mode_value() {
        let src = r#"
            export function EditForm() {
              const form = useForm({ mode: "onChange", reValidateMode: "onChange" });
              return <form />;
            }
        "#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("onTouched"));
        assert!(!d[0].message.contains("reValidateMode"));
    }

    #[test]
    fn flags_missing_revalidatemode() {
        let src = r#"
            export function EditForm() {
              const form = useForm({ mode: "onTouched" });
              return <form />;
            }
        "#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("reValidateMode"));
        assert!(!d[0].message.contains("`mode"));
    }

    #[test]
    fn flags_wrong_revalidatemode_value() {
        let src = r#"
            export function EditForm() {
              const form = useForm({ mode: "onTouched", reValidateMode: "onBlur" });
              return <form />;
            }
        "#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("reValidateMode"));
        assert!(!d[0].message.contains("`mode"));
    }

    #[test]
    fn allows_correct_config() {
        let src = r#"
            export function EditForm() {
              const form = useForm({ mode: "onTouched", reValidateMode: "onChange" });
              return <form />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_correct_config_with_other_options() {
        let src = r#"
            export function EditForm() {
              const form = useForm({
                resolver: zodResolver(schema),
                mode: "onTouched",
                reValidateMode: "onChange",
                defaultValues: { name: "" },
              });
              return <form />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_spread_config() {
        // A spread can supply either option; can't verify statically.
        let src = r#"
            export function EditForm() {
              const form = useForm({ ...baseConfig });
              return <form />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_variable_config() {
        // The config lives in a variable; can't verify statically.
        let src = r#"
            export function EditForm() {
              const form = useForm(formConfig);
              return <form />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_literal_values() {
        // Computed/const values are trusted; flagging them would be a false positive.
        let src = r#"
            export function EditForm() {
              const form = useForm({ mode: MODE, reValidateMode: REVALIDATE });
              return <form />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_use_form_context() {
        let src = r#"
            export function Field() {
              const { register } = useFormContext();
              return <input {...register("x")} />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_tanstack_react_form_useform() {
        // Regression for rbaumier/comply#1594 — `@tanstack/react-form`'s
        // `useForm` has no `mode`/`reValidateMode` options; this RHF rule must
        // not fire on it.
        let src = r#"
            import { useForm } from '@tanstack/react-form';
            export default function App() {
              const form = useForm({
                defaultValues: { firstName: '', lastName: '' },
                onSubmit: async ({ value }) => { console.log(value); },
              });
              return <form />;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_react_hook_form_useform_with_import() {
        // Negative space: a genuine react-hook-form `useForm` still fires.
        let src = r#"
            import { useForm } from 'react-hook-form';
            export function EditForm() {
              const form = useForm({ resolver: zodResolver(schema) });
              return <form />;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
