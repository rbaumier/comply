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

        // Only flag when the value side is an object or array literal.
        let is_literal = matches!(
            &as_expr.expression,
            Expression::ObjectExpression(_) | Expression::ArrayExpression(_)
        );
        if !is_literal {
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
mod tests {
    use super::*;

    fn run_ts(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    fn run_tsx(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }

    #[test]
    fn flags_object_literal_cast() {
        assert_eq!(run_ts("const x = { a: 1 } as Config;").len(), 1);
    }

    #[test]
    fn flags_array_literal_cast() {
        assert_eq!(run_ts("const y = [1, 2] as Tuple;").len(), 1);
    }

    #[test]
    fn allows_non_literal_cast() {
        assert!(run_ts("const x = foo as Config;").is_empty());
    }

    #[test]
    fn allows_as_const() {
        assert!(run_ts("const x = [1, 2] as const;").is_empty());
    }

    #[test]
    fn allows_satisfies() {
        assert!(run_ts("const x = { a: 1 } satisfies Config;").is_empty());
    }

    // Regression test for #569: `as React.CSSProperties` on an object with
    // CSS custom properties is necessary — `satisfies` would fail to compile
    // because @types/react has no index signature for `--*` keys.
    #[test]
    fn allows_css_custom_props_as_react_css_properties() {
        assert!(run_tsx(
            r#"import type React from 'react';
const style = { "--my-var": "100px" } as React.CSSProperties;"#
        )
        .is_empty());
    }

    #[test]
    fn allows_css_custom_props_with_spread() {
        assert!(run_tsx(
            r#"import type React from 'react';
const s = {
    "--sidebar-width": "200px",
    "--sidebar-width-icon": "48px",
    ...extra,
} as React.CSSProperties;"#
        )
        .is_empty());
    }

    #[test]
    fn still_flags_react_css_properties_without_custom_props() {
        assert_eq!(
            run_tsx("const s = { color: 'red' } as React.CSSProperties;").len(),
            1
        );
    }
}
