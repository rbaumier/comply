//! no-multi-op-oneliner oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Test assertions (Playwright, Vitest) are inherently chained.
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        let min_ops = ctx
            .config
            .threshold("no-multi-op-oneliner", "min_ops", ctx.lang);
        let min_line_length = ctx
            .config
            .threshold("no-multi-op-oneliner", "min_line_length", ctx.lang);

        let line_offsets = super::dense_lines::compute_line_offsets(ctx.source);

        // Collect byte ranges to strip before counting operators: comments and
        // regex literals. A regex pattern's `-`, `/`, and quantifier syntax are
        // not chained operations — stripping them mirrors the string-literal skip
        // already done in `count_operators`.
        let mut strip_ranges: Vec<(usize, usize)> = semantic
            .comments()
            .iter()
            .map(|c| (c.span.start as usize, c.span.end as usize))
            .collect();
        for node in semantic.nodes() {
            if let AstKind::RegExpLiteral(re) = node.kind() {
                strip_ranges.push((re.span.start as usize, re.span.end as usize));
            }
        }

        let mut reported_lines = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();

        for node in semantic.nodes() {
            let is_target = matches!(
                node.kind(),
                AstKind::ExpressionStatement(_) | AstKind::VariableDeclarator(_)
            );
            if !is_target {
                continue;
            }
            let span = match node.kind() {
                AstKind::ExpressionStatement(s) => s.span,
                AstKind::VariableDeclarator(d) => d.span,
                _ => continue,
            };
            let (start_line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
            let (end_line, _) = byte_offset_to_line_col(ctx.source, span.end as usize);
            if start_line != end_line {
                continue;
            }
            if reported_lines.contains(&start_line) {
                continue;
            }
            // line_offsets is 0-indexed, start_line is 1-indexed.
            let Some(&(line_start_byte, line_text)) = line_offsets.get(start_line - 1) else {
                continue;
            };
            let stripped =
                super::dense_lines::strip_comments(line_text, line_start_byte, &strip_ranges);
            if stripped.len() < min_line_length {
                continue;
            }
            let ops = super::dense_lines::count_operators(&stripped);
            if ops < min_ops {
                continue;
            }
            reported_lines.insert(start_line);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: start_line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Line has {ops} chained operations — extract intermediate \
                     named bindings so each step's purpose is visible."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    #[test]
    fn ignores_regex_literal_character_classes() {
        // Regression for issue #524: a regex literal's `-`, `/` and quantifier
        // syntax are not chained operations.
        let src = "const UUIDV7_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$/;";
        assert!(run_oxc_ts(src, &Check).is_empty(), "got {:?}", run_oxc_ts(src, &Check));
    }

    #[test]
    fn still_flags_dense_operator_chain() {
        let src = "const total = items.map(x => x.price).filter(p => p > 0).reduce((a, b) => a + b, 0) + extra;";
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_heavy_oneliner() {
        let source = "const total = items.filter(i => i.active).map(i => i.price).reduce((a, b) => a + b, 0) * tax + discount;";
        assert_eq!(run_on(source).len(), 1);
    }


    #[test]
    fn allows_simple_oneliner() {
        assert!(run_on("const x = a + b;").is_empty());
    }


    #[test]
    fn allows_short_but_dense_expression() {
        // Dense but short — under the line-length floor.
        assert!(run_on("const x = a.b.c + d.e * f;").is_empty());
    }


    #[test]
    fn does_not_count_operators_inside_trailing_line_comment() {
        // TS equivalent of the Rust FP from RULES_TO_FIX.md #6.
        let source = "expect(run(\"utils.spec.ts\", \"// TODO: add tests\").length).toBe(1); // eslint-disable-next-line — test content not a real marker";
        assert!(run_on(source).is_empty());
    }


    #[test]
    fn does_not_count_operators_inside_trailing_block_comment() {
        let source = "const x = a + b; /* note: a - b * c / d - e + f */";
        assert!(run_on(source).is_empty());
    }


    #[test]
    fn still_flags_dense_code_with_trailing_comment() {
        let source = "const total = items.filter(i => i.active).map(i => i.price).reduce((a, b) => a + b, 0) * tax + discount; // total";
        assert_eq!(run_on(source).len(), 1);
    }


    #[test]
    fn does_not_flag_short_code_with_long_trailing_comment() {
        let source = "const x = a + b; // a fairly long explanation that the result is the sum of a and b and not anything more interesting at all";
        assert!(run_on(source).is_empty());
    }
}
