use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_FILE_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_FILE_MARKERS.iter().any(|m| s.contains(m))
}

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
        if is_test_file(ctx.path) {
            return;
        }

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

        // A plain anchored literal like `/^Type invalide : chaîne…$/`
        // is its own documentation — adding a comment that restates the
        // sentence verbatim would be pure noise.
        if super::is_simple_anchored_literal(&pattern) {
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
        let row = probe_line.saturating_sub(1);
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Blank lines between comment and declaration still count as documentation.
        let mut probe = row;
        while probe > 0 {
            probe -= 1;
            let Some(text) = lines.get(probe) else { break };
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with("*")
            {
                return;
            }
            break;
        }

        let has_comment_before = semantic.comments().iter().any(|c| {
            let (cline, _) = byte_offset_to_line_col(ctx.source, c.span.start as usize);
            cline == line || cline + 1 == line
        });
        if has_comment_before {
            return;
        }

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Complex regex without a comment — add a description of what it matches.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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
        crate::rules::test_helpers::run_rule(&Check, src, path)
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
    fn still_flags_in_non_test_file() {
        let src = r#"const r = /^[a-z]+@[a-z]+\.[a-z]{2,4}$/;"#;
        assert_eq!(run_with_path(src, "src/auth.ts").len(), 1);
    }
}
