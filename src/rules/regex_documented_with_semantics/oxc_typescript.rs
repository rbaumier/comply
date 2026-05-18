use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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

        // A plain anchored literal like `/^Type invalide : chaîne…$/`
        // is its own documentation — adding a comment that restates the
        // sentence verbatim would be pure noise.
        if super::is_simple_anchored_literal(&pattern) {
            return;
        }

        let (line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
        let row = line.saturating_sub(1);
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
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
}
