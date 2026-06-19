//! zod-prefer-discriminated-union OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const TAG_KEYS: &[&str] = &["type", "kind", "__type"];

/// Check if an `z.object({...})` call argument contains a tag field with `z.literal(...)` value.
fn object_has_tag_literal(args: &oxc_ast::ast::CallExpression, source: &str) -> bool {
    // First argument should be an object expression
    let Some(first_arg) = args.arguments.first() else {
        return false;
    };
    let Argument::ObjectExpression(obj) = first_arg else {
        return false;
    };
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop else {
            continue;
        };
        let key_text = &source[prop.key.span().start as usize..prop.key.span().end as usize];
        let normalized = key_text.trim_matches(|c: char| c == '"' || c == '\'');
        if !TAG_KEYS.contains(&normalized) {
            continue;
        }
        // Value must be a call to z.literal(...)
        let Expression::CallExpression(value_call) = &prop.value else {
            continue;
        };
        let callee_text =
            &source[value_call.callee.span().start as usize..value_call.callee.span().end as usize];
        if callee_text == "z.literal" {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.union"])
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

        // A perf-optimization suggestion has no place in a benchmark file, which
        // exists to compare `z.union` against `z.discriminatedUnion` — the
        // `z.union` is the deliberate comparison baseline, not a mistake.
        if ctx.file.in_benchmark_dir() {
            return;
        }

        // Callee must be z.union
        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "z.union" {
            return;
        }

        // First argument must be an array
        let Some(Argument::ArrayExpression(array)) = call.arguments.first() else {
            return;
        };

        // Check if any array element is a z.object with a tag literal
        let mut has_literal_tag = false;
        for elem in &array.elements {
            let oxc_ast::ast::ArrayExpressionElement::CallExpression(elem_call) = elem else {
                continue;
            };
            let elem_callee =
                &ctx.source[elem_call.callee.span().start as usize..elem_call.callee.span().end as usize];
            if elem_callee != "z.object" {
                continue;
            }
            if object_has_tag_literal(elem_call, ctx.source) {
                has_literal_tag = true;
                break;
            }
        }

        if !has_literal_tag {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Replace `z.union([z.object({type: z.literal(...)}), ...])` with `z.discriminatedUnion('type', [...])` for faster parsing.".into(),
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

    // Routes through a built `FileCtx` so the benchmark-dir gate is exercised
    // exactly as in a real run (`in_benchmark_dir()` is only populated there).
    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, source, path)
    }

    // The union-of-objects-with-shared-`z.literal`-discriminant shape from
    // issue #4435's benchmark file, kept here as the single source repro.
    const BENCH_REPRO: &str = r#"
        const z4Union = z.union([
            z.object({ type: z.literal("a"), value: z.string() }),
            z.object({ type: z.literal("b"), value: z.number() }),
        ]);
    "#;

    // Issue #4435: a perf suggestion inside a benchmark file is a false positive
    // — the `z.union` is the deliberate baseline of a `z.union` vs
    // `z.discriminatedUnion` comparison.
    #[test]
    fn skips_union_in_benchmark_file() {
        assert!(run_at(BENCH_REPRO, "packages/bench/discriminated-union.ts").is_empty());
    }

    // Load-bearing: the identical shape in a production file is still flagged —
    // the benchmark gate must not erase the rule's purpose.
    #[test]
    fn flags_union_in_production_file() {
        assert_eq!(run_at(BENCH_REPRO, "src/schema.ts").len(), 1);
    }
}
