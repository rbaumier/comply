//! OxcCheck backend for zod-prefer-overwrite-v4.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["transform"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `*.transform`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "transform" {
            return;
        }

        // Find the arrow function argument
        let Some(arrow_arg) = call.arguments.first() else {
            return;
        };
        let Argument::ArrowFunctionExpression(arrow) = arrow_arg else {
            return;
        };

        // Extract single parameter name
        let arrow_src =
            &ctx.source[arrow.span.start as usize..arrow.span.end as usize];
        let Some(param) = extract_single_param(arrow_src) else {
            return;
        };
        let param = param.to_string();

        let same_shape = if arrow.expression {
            // Expression body
            let body_src = if let Some(stmt) = arrow.body.statements.first() {
                if let Statement::ExpressionStatement(es) = stmt {
                    let start = es.expression.span().start as usize;
                    let end = es.expression.span().end as usize;
                    Some(&ctx.source[start..end])
                } else {
                    None
                }
            } else {
                None
            };
            body_src.is_some_and(|s| is_same_shape_expr(s, &param))
        } else {
            // Block body — look for a single return
            let returns: Vec<&str> = arrow
                .body
                .statements
                .iter()
                .filter_map(|s| {
                    if let Statement::ReturnStatement(ret) = s {
                        ret.argument.as_ref().map(|e| {
                            &ctx.source[e.span().start as usize..e.span().end as usize]
                        })
                    } else {
                        None
                    }
                })
                .collect();
            returns.len() == 1 && is_same_shape_expr(returns[0], &param)
        };

        if !same_shape {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.transform()` returns the same-shape value as its input — \
                      use `.overwrite()` (Zod v4) to keep the input type intact."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn extract_single_param(arrow_src: &str) -> Option<&str> {
    let arrow_idx = arrow_src.find("=>")?;
    let head = arrow_src[..arrow_idx].trim();
    let head = head.strip_prefix("async").map(str::trim).unwrap_or(head);
    if let Some(rest) = head.strip_prefix('(') {
        let inner = rest.strip_suffix(')')?.trim();
        if inner.is_empty() || inner.contains(',') {
            return None;
        }
        let name = inner.split(':').next()?.trim();
        if !is_ident(name) {
            return None;
        }
        Some(name)
    } else {
        if is_ident(head) {
            Some(head)
        } else {
            None
        }
    }
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Method names on `param` that REFINE without changing type — these
/// are shape-preserving (string→string, number→number…). Anything else
/// called on `param` (`.toISOString()`, `.toString()`, `.toFixed()`,
/// `.toLocaleString()`, `.valueOf()`, `.charAt()`, …) is shape-changing
/// and must NOT be treated as same-shape.
const SHAPE_PRESERVING_METHODS: &[&str] = &[
    "trim",
    "trimStart",
    "trimEnd",
    "padStart",
    "padEnd",
    "replace",
    "replaceAll",
    "slice",
    "substring",
    "substr",
    "toUpperCase",
    "toLowerCase",
    "normalize",
];

/// True if `s` is `param.<method>(...)` where `<method>` preserves the
/// runtime type of `param`. Excludes type-changing methods like
/// `.toISOString()`.
fn is_same_shape_method_call(s: &str, param: &str) -> bool {
    let Some(rest) = s.strip_prefix(param) else {
        return false;
    };
    let Some(after_dot) = rest.strip_prefix('.') else {
        return false;
    };
    // Pull the method identifier (everything up to `(` or end).
    let method_end = after_dot
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(after_dot.len());
    let method = &after_dot[..method_end];
    SHAPE_PRESERVING_METHODS.contains(&method)
}

fn is_same_shape_expr(expr_text: &str, param: &str) -> bool {
    let t = expr_text.trim().trim_end_matches(';');
    if t == param {
        return true;
    }
    if is_same_shape_method_call(t, param) {
        return true;
    }
    for fun in [
        "Math.round",
        "Math.floor",
        "Math.ceil",
        "Math.abs",
        "Math.trunc",
    ] {
        if t.starts_with(fun) && t.contains(param) {
            return true;
        }
    }
    if let Some(rest) = t.strip_prefix(param) {
        let rest = rest.trim_start();
        for op in ['+', '-', '*', '/'] {
            if let Some(rhs) = rest.strip_prefix(op) {
                let rhs = rhs.trim();
                if !rhs.is_empty() && rhs.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    return true;
                }
            }
        }
    }
    false
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_same_shape_transform() {
        // Truly same-shape: trimming a string stays a string.
        let src = "const s = z.string().transform((s) => s.trim());";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_identity_transform() {
        let src = "const s = z.string().transform(s => s);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_type_changing_transform_to_iso_string() {
        // Regression for rbaumier/comply#20 — Date -> string.
        let src = "const s = z.date().transform(d => d.toISOString());";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_to_fixed_transform() {
        let src = "const s = z.number().transform(n => n.toFixed(2));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_nullish_default_substitution_issue_4201() {
        // Nullish coalescing removes null from a nullable schema → shape-changing → not an
        // `.overwrite()` candidate; the `param ?? X` shape is ambiguous w.r.t. upstream
        // nullability so it is not flagged.
        let src =
            "const s = z.string().nullable().default(null).transform((v) => v ?? fallback);";
        assert!(run(src).is_empty());
    }
}
