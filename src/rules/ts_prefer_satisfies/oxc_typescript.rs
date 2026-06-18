//! ts-prefer-satisfies oxc backend — flag `{...} as T` / `[...] as T`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else { return };

        // Test stubs cast partial literals to full library types where
        // `satisfies` is impossible (the target has required fields the stub
        // omits) — skip test files.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // Only flag when the value side is an object or array literal.
        let is_literal = matches!(
            &as_expr.expression,
            Expression::ObjectExpression(_) | Expression::ArrayExpression(_)
        );
        if !is_literal {
            return;
        }

        // An empty object literal can never satisfy a type with required
        // members, so `{} satisfies T` fails to compile (TS1360). `{} as T`
        // is the canonical deliberate-coercion idiom (default/fallback props,
        // empty option bags) — skip it. An empty *array* is exempt from this:
        // `[] satisfies T[]` compiles, so it still flags below.
        if let Expression::ObjectExpression(obj) = &as_expr.expression
            && obj.properties.is_empty()
        {
            return;
        }

        // Filter out `as const`.
        let type_text = &ctx.source[as_expr.type_annotation.span().start as usize..as_expr.type_annotation.span().end as usize];
        if type_text.trim() == "const" {
            return;
        }

        // `as React.CSSProperties` on an object containing CSS custom property
        // keys (`--*`) is the documented workaround: @types/react removed the
        // index signature, so `satisfies React.CSSProperties` would fail to
        // compile when any key starts with `--`.
        if type_text.trim() == "React.CSSProperties" {
            if let Expression::ObjectExpression(obj) = &as_expr.expression {
                let has_css_custom_prop = obj.properties.iter().any(|prop| {
                    if let ObjectPropertyKind::ObjectProperty(p) = prop {
                        p.key.static_name().is_some_and(|k| k.starts_with("--"))
                    } else {
                        false
                    }
                });
                if has_css_custom_prop {
                    return;
                }
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`as Type` on a literal widens the inferred type — use `satisfies Type` to validate without widening.".into(),
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

    fn run_ts(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_object_literal_cast() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1 } as Config;", "t.ts").len(), 1);
    }

    #[test]
    fn allows_stub_cast_in_test_files() {
        // Regression for issue #573: partial stubs can't use `satisfies`.
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, "const a = { api: { getSession: async () => null } } as Auth;", "t.tsx", crate::project::default_static_project_ctx(), &file)
            .is_empty()
        );
    }

    #[test]
    fn flags_array_literal_cast() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const y = [1, 2] as Tuple;", "t.ts").len(), 1);
    }

    // Regression test for #3881: an empty object literal cast cannot use
    // `satisfies` (`{} satisfies T` is TS1360 when T has required members),
    // so `{} as T` must not be flagged.
    #[test]
    fn allows_empty_object_literal_cast() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = {} as T;", "t.ts").is_empty());
    }

    #[test]
    fn allows_empty_object_fallback_cast() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const o = pOptions || ({} as ResourceOptions<T, S>);", "t.ts")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_non_empty_object_literal_cast() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1 } as T;", "t.ts").len(), 1);
    }

    // An empty array still flags: `[] satisfies T[]` compiles fine, so the
    // empty-object exemption must not extend to arrays.
    #[test]
    fn still_flags_empty_array_literal_cast() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const a = [] as T[];", "t.ts").len(), 1);
    }

    #[test]
    fn allows_non_literal_cast() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = foo as Config;", "t.ts").is_empty());
    }

    #[test]
    fn allows_as_const() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = [1, 2] as const;", "t.ts").is_empty());
    }

    #[test]
    fn allows_satisfies() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1 } satisfies Config;", "t.ts").is_empty());
    }

    // Regression test for #569: `as React.CSSProperties` on an object with
    // CSS custom properties is necessary — `satisfies` would fail to compile
    // because @types/react has no index signature for `--*` keys.
    #[test]
    fn allows_css_custom_props_as_react_css_properties() {
        assert!(crate::rules::test_helpers::run_rule(&Check, r#"import type React from 'react';
const style = { "--my-var": "100px" } as React.CSSProperties;"#, "t.tsx")
        .is_empty());
    }

    #[test]
    fn allows_css_custom_props_with_spread() {
        assert!(crate::rules::test_helpers::run_rule(&Check, r#"import type React from 'react';
const s = {
    "--sidebar-width": "200px",
    "--sidebar-width-icon": "48px",
    ...extra,
} as React.CSSProperties;"#, "t.tsx")
        .is_empty());
    }

    #[test]
    fn still_flags_react_css_properties_without_custom_props() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const s = { color: 'red' } as React.CSSProperties;", "t.tsx").len(),
            1
        );
    }
}
