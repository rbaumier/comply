//! use-type-alias OxcCheck backend — detect repeated complex inline type
//! annotations via oxc AST.
//!
//! Two-pass via `run_on_semantic`: iterate all nodes collecting union/intersection
//! type text, then report duplicates.

use std::collections::HashMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;

pub struct Check;

/// True if `t` is a single keyword type (`string`, `number`, …) or a
/// plain type-reference identifier — i.e. a one-token type without
/// nested structure.
fn is_simple_type(t: &TSType) -> bool {
    matches!(
        t,
        TSType::TSNullKeyword(_)
            | TSType::TSUndefinedKeyword(_)
            | TSType::TSStringKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSBooleanKeyword(_)
            | TSType::TSBigIntKeyword(_)
            | TSType::TSAnyKeyword(_)
            | TSType::TSUnknownKeyword(_)
            | TSType::TSNeverKeyword(_)
            | TSType::TSObjectKeyword(_)
            | TSType::TSVoidKeyword(_)
            | TSType::TSSymbolKeyword(_)
            | TSType::TSTypeReference(_)
    )
}

fn is_null_or_undefined(t: &TSType) -> bool {
    matches!(
        t,
        TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_)
    )
}

/// A pattern like `T | null`, `T | undefined`, `null | undefined` —
/// short, structurally trivial, and almost always semantically distinct
/// at each call site (a nullable DSN is a different concept from a
/// nullable CSP host). Promoting these to a shared alias hurts more
/// than it helps.
fn is_trivial_nullable_union(types: &[TSType]) -> bool {
    if types.len() != 2 {
        return false;
    }
    if !types.iter().all(is_simple_type) {
        return false;
    }
    types.iter().any(is_null_or_undefined)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut annotation_lines: HashMap<String, Vec<usize>> = HashMap::new();

        for node in semantic.nodes().iter() {
            let span = match node.kind() {
                AstKind::TSUnionType(u) => {
                    // A trivial nullable union (`T | null`, `T | undefined`)
                    // is rarely a shared domain concept — counting it
                    // produces a steady stream of "rename to StringOrNull"
                    // suggestions that destroy local readability.
                    if is_trivial_nullable_union(&u.types) {
                        continue;
                    }
                    u.span
                }
                AstKind::TSIntersectionType(i) => i.span,
                _ => continue,
            };

            // Skip nested union/intersection — only count the outermost.
            let parent = semantic.nodes().parent_node(node.id());
            if matches!(parent.kind(), AstKind::TSUnionType(_) | AstKind::TSIntersectionType(_)) {
                continue;
            }

            let text = &ctx.source[span.start as usize..span.end as usize];
            if text.len() <= 5 {
                continue;
            }

            let (line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
            annotation_lines
                .entry(text.to_string())
                .or_default()
                .push(line);
        }

        let mut diagnostics = Vec::new();
        for (annotation, lines) in &annotation_lines {
            if lines.len() >= 2 {
                for &line_num in lines {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: line_num,
                        column: 1,
                        rule_id: "use-type-alias".into(),
                        message: format!(
                            "Inline type `{}` appears {} times \u{2014} extract a type alias.",
                            annotation,
                            lines.len()
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics.sort_by_key(|d| d.line);
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_repeated_complex_union() {
        let src = r#"
            const a: string | number | boolean = 1 as any;
            const b: string | number | boolean = 2 as any;
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_repeated_nullable_union() {
        // Regression for rbaumier/comply#31 — `string | null` is too
        // generic to share an alias for; distinct call sites are nearly
        // always semantically distinct concepts.
        let src = r#"
            export type Config = { sentryDsn: string | null };
            type CspConnectSource = string | null;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_repeated_optional_union() {
        let src = r#"
            type A = number | undefined;
            type B = number | undefined;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_nullish_pair() {
        let src = r#"
            type A = null | undefined;
            type B = null | undefined;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_nullable_with_complex_other_side() {
        // `{ a: string } | null` is structurally distinctive — the
        // object-literal side is what makes extraction worthwhile.
        let src = r#"
            type A = { a: string } | null;
            type B = { a: string } | null;
            type C = { a: string } | null;
        "#;
        assert!(!run(src).is_empty());
    }
}
