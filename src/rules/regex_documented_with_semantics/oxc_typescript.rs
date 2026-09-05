use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::comment_blocks::{closes_its_row, is_trailing};
use std::sync::Arc;

/// When `node` is the direct initializer of a `const`/`let`/`var` declarator,
/// return the 1-based line of the enclosing `VariableDeclaration`. The regex
/// literal may live on a continuation line below `const X =`, so a doc comment
/// above the declaration documents it. Returns `None` when the regex is nested
/// (array element, call argument, object property…), keeping the literal's own
/// line so an unrelated comment above the enclosing statement never counts.
fn enclosing_declaration_line<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> Option<usize> {
    let nodes = semantic.nodes();
    if !matches!(nodes.parent_node(node.id()).kind(), AstKind::VariableDeclarator(_)) {
        return None;
    }
    for ancestor in nodes.ancestors(node.id()) {
        if let AstKind::VariableDeclaration(decl) = ancestor.kind() {
            let (decl_line, _) = byte_offset_to_line_col(source, decl.span.start as usize);
            return Some(decl_line);
        }
    }
    None
}

/// True when a comment of the file documents what sits on one of `anchors`,
/// the 1-based rows the regex can be read from.
///
/// Two positions qualify, both settled by the comment's own span:
///
/// - leading — it closes above the anchor with only blank rows in between, and
///   it opens its own row. One sharing its row with earlier code labels that
///   code: `const x = f(); // why` documents the call, not the row below.
/// - trailing — it opens on the anchor row and closes it. One with code after
///   it on that row annotates that code: the `/* flag */` of
///   `check(re, /* flag */ true)` describes `true`.
///
/// The caller anchors on the enclosing declaration's row and on the regex's own
/// row. They differ when the literal sits on a continuation line below
/// `const X =`, where a comment documents it from either side of the `=`.
fn has_documenting_comment(
    semantic: &oxc_semantic::Semantic<'_>,
    source: &str,
    anchors: [usize; 2],
) -> bool {
    let rows: Vec<&str> = source.lines().collect();
    let is_blank = |row: usize| rows.get(row - 1).is_some_and(|text| text.trim().is_empty());
    semantic.comments().iter().any(|comment| {
        let (start, end) = (comment.span.start as usize, comment.span.end as usize);
        let (opens_on, _) = byte_offset_to_line_col(source, start);
        let (closes_on, _) = byte_offset_to_line_col(source, end);
        anchors.iter().any(|&anchor| {
            let leads = closes_on < anchor
                && !is_trailing(source, start)
                && (closes_on + 1..anchor).all(&is_blank);
            let trails = opens_on == anchor && closes_its_row(source, end);
            leads || trails
        })
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (span, pattern) = match node.kind() {
            AstKind::RegExpLiteral(re) => {
                (re.span, re.regex.pattern.text.as_str().to_string())
            }
            AstKind::CallExpression(call) => {
                let is_new_regexp = match &call.callee {
                    oxc_ast::ast::Expression::NewExpression(new_expr) => {
                        matches!(&new_expr.callee, oxc_ast::ast::Expression::Identifier(id) if id.name.as_str() == "RegExp")
                    }
                    _ => false,
                };
                if !is_new_regexp {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else { return };
                let pat = match first_arg {
                    oxc_ast::ast::Argument::StringLiteral(s) => s.value.as_str().to_string(),
                    _ => return,
                };
                (call.span, pat)
            }
            _ => return,
        };

        if pattern.len() < super::MIN_PATTERN_LEN {
            return;
        }

        // A pattern that matches literal text only — `/^Type invalide : chaîne…$/`,
        // `/jiti|node:internal|citty/`, `/Error: sendMessage\(\) cannot be used/` —
        // is its own documentation; a comment restating it would be pure noise.
        if super::is_literal_pattern(&pattern) {
            return;
        }

        let (line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);

        // When the regex is the direct initializer of a `const X =\n  /re/`
        // declaration, the literal can sit on a continuation line below the
        // declaration. Anchor the doc-comment probe at the enclosing
        // declaration so a comment above `const X =` documents the regex, just
        // as it would when the literal shares the line with `=`. A nested regex
        // (array element, call argument, object property…) keeps its own line,
        // so an unrelated comment above the enclosing statement never counts.
        let probe_line = enclosing_declaration_line(node, semantic, ctx.source).unwrap_or(line);
        if has_documenting_comment(semantic, ctx.source, [probe_line, line]) {
            return;
        }

        // Anchor on the literal itself, not on the left margin: two regexes
        // sharing a line are two findings, and only the column tells them apart.
        diagnostics.push(Diagnostic::at_offset(
            Arc::clone(&ctx.path_arc),
            ctx.source,
            (span.start as usize, span.size() as usize),
            super::META.id,
            "Complex regex without a comment — add a description of what it matches.".into(),
            Severity::Error,
        ));
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn anchors_each_literal_of_a_shared_line_on_its_own_column() {
        // Regression for rbaumier/comply#8386 — two undocumented regexes on one
        // line used to produce two records identical in every serialized field.
        let src = r#"const rs = [/^[a-z]+@[a-z]+\.[a-z]{2,4}$/, /^[0-9]{3}-[0-9]{4}-[a-z]+$/];"#;
        let diags = run(src);
        let positions: Vec<(usize, usize)> = diags.iter().map(|d| (d.line, d.column)).collect();
        assert_eq!(positions, vec![(1, 13), (1, 44)]);
        for d in &diags {
            let (offset, len) = d.span.expect("the anchor carries the literal's span");
            assert!(src[offset..offset + len].starts_with('/'));
        }
    }

    #[test]
    fn flags_complex_regex_without_comment() {
        let src = r#"const r = /^[a-z]+@[a-z]+\.[a-z]{2,4}$/;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_complex_regex_with_comment_above() {
        let src = r#"
            // email-shaped string
            const r = /^[a-z]+@[a-z]+\.[a-z]{2,4}$/;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_anchored_literal_sentence() {
        // Regression for rbaumier/comply#19 — a `/^…$/` regex with no
        // metacharacters IS its own documentation.
        let src = r#"const r = /^Type invalide : chaîne attendu, nombre reçu$/;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_block_comment_above_const_declaration() {
        // Regression for rbaumier/comply#102 — a multi-line `//` comment
        // block describing the regex semantics, separated from the `const`
        // declaration by a blank line, should still count as documentation
        // for the regex inside the declaration.
        let src = "// Two regexes, intentionally separate so each one is plain and self-explanatory.\n//\n// `BODY_RESPONSE_CALL_RE` — Elysia's legacy fluent shape: a `.body(...)` or\n// `.response(...)` method call whose first token is `z.object(` / `z.strictObject(`.\n\nconst BODY_RESPONSE_CALL_RE = /\\.(?:body|response)\\(\\s*z\\.(?:object|strictObject)\\(/;\n";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn allows_multi_line_block_comment_above() {
        // Regression for rbaumier/comply#8326 — the closing line of a `/* … */`
        // block starts with neither `//`, `/*` nor `*`, so a raw-text scan
        // stopped there and flagged a documented regex.
        let src = "/* ISO 8601 duration:\n   years, months, days */\nexport const A = /^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$/;\n";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn still_flags_when_previous_statement_carries_a_trailing_comment() {
        // Regression for rbaumier/comply#8326 — the comment documents the
        // statement it trails, so it says nothing about the regex below it.
        let src = "const trimmed = \"x\".trim(); // strip surrounding blanks\nexport const B = /^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$/;\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_when_a_mid_line_block_comment_annotates_another_argument() {
        // Regression for rbaumier/comply#8326 — `/* caseInsensitive */` labels
        // the `true` that follows it, not the regex that precedes it.
        let src = "export const E = check(/^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$/, /* caseInsensitive */ true);\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_trailing_block_comment_on_the_regex_line() {
        let src = "export const C = /^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$/; /* dark magic */\n";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn allows_comment_between_the_declaration_and_a_continuation_line_regex() {
        let src = "export const X =\n  // matches static import statements\n  /import\\s+[\"']([^\"']+)[\"']/gmu;\n";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn still_flags_when_only_unrelated_code_precedes() {
        // Sanity check: skipping blank lines must not reach past real code
        // and treat a far-away comment as documentation for this regex.
        let src = "// A comment about something unrelated.\nconst other = 1;\n\nconst r = /^[a-z]+@[a-z]+\\.[a-z]{2,4}$/;\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_jsdoc_above_continuation_line_regex_issue_6518() {
        // Regression for rbaumier/comply#6518 — a JSDoc block immediately above
        // `export const X =` documents the regex even when the literal sits on
        // the continuation line below the `=`, where the probe used to stop at
        // the declaration line and miss the comment.
        let src = r#"
/**
 * Regular expression to match static import statements.
 * @example `import { foo } from 'module'`
 */
export const ESM_STATIC_IMPORT_RE =
  /import\s+["']([^"']+)["']/gmu;
"#;
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn still_flags_continuation_line_regex_without_comment() {
        // Anchoring the probe at the enclosing declaration must not silence a
        // genuinely undocumented complex regex on a continuation line.
        let src = r#"export const X =
  /import\s+["']([^"']+)["']/gmu;
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_anchored_literal_in_vitest_table() {
        let src = r#"
            const cases = [
              { match: /^Type invalide : chaîne attendu, nombre reçu$/ },
              { match: /^Type invalide : nombre attendu, chaîne reçu$/ },
              { match: /^Type invalide : boolean attendu, chaîne reçu$/ },
            ];
        "#;
        assert!(run(src).is_empty());
    }

    fn run_with_path(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path)
    }

    #[test]
    fn skips_complex_regex_in_test_file() {
        // Regression for #384 — regexes in test files (grep args, toMatch, etc.)
        // are test-internal and should not require JSDoc.
        let src = r#"const result = execSync("grep -r 'x' src/").toString();
expect(result).toMatch(/^[a-z]+@[a-z]+\.[a-z]{2,4}$/);"#;
        assert!(run_with_path(src, "src/auth.test.ts").is_empty());
        assert!(run_with_path(src, "src/auth.spec.ts").is_empty());
        assert!(run_with_path(src, "__tests__/auth.ts").is_empty());
        assert!(run_with_path(src, "src/auth_test.ts").is_empty());
    }

    #[test]
    fn skips_complex_regex_under_a_test_directory() {
        // Regression for rbaumier/comply#8243 — a project that keeps its tests in
        // a `test/` directory instead of co-locating them was flagged, so the same
        // regex got opposite verdicts depending on how the file is marked.
        let src = r#"const result = execSync("grep -r 'x' src/").toString();
expect(result).toMatch(/^[a-z]+@[a-z]+\.[a-z]{2,4}$/);"#;
        for path in [
            "test/direction.js",
            "tests/direction.js",
            "e2e/direction.js",
            "test-helpers/direction.js",
            "src/direction.test.ts",
        ] {
            assert!(
                run_with_path(src, path).is_empty(),
                "expected no diagnostics under {path}"
            );
        }
    }

    #[test]
    fn still_flags_in_non_test_file() {
        let src = r#"const r = /^[a-z]+@[a-z]+\.[a-z]{2,4}$/;"#;
        assert_eq!(run_with_path(src, "src/auth.ts").len(), 1);
    }

    #[test]
    fn ignores_escaped_punctuation_literal() {
        // Regression for rbaumier/comply#8243 — `\(` matches a parenthesis, so
        // the pattern spells a plain sentence: there is nothing to document that
        // the pattern does not already say.
        let src = r#"export const A = /Error: sendMessage\(\) cannot be used/;
export const B = /`stdout\.input` option must use a boolean/;"#;
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn still_flags_escapes_that_carry_semantics() {
        // `\d` is a class, not an escaped literal — the pattern still needs a
        // comment even though it is spelled with backslashes.
        let src = r#"export const C = /^fd(?<fdNumber>\d+)$/;
export const T = /\d{2}:\d{2} something long enough/;"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn ignores_pure_literal_alternation() {
        // Regression for rbaumier/comply#6645 — a `|`-alternation of plain
        // literals reads as "match any of these exact strings" and is its
        // own documentation regardless of total length.
        let src = r#"const InternalStackRe = /jiti|node:internal|citty|listhen|listenAndWatch/;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_alternation_with_metacharacter_alternative() {
        let src = r#"const r = /foo|ba.r|something_long_enough/;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_quantified_group() {
        let src = r#"const r = /(a|b)+ something long enough here/;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_character_class() {
        let src = r#"const r = /[a-z]+ something long enough to trip/;"#;
        assert_eq!(run(src).len(), 1);
    }
}
