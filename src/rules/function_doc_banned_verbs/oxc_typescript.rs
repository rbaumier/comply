//! function-doc-banned-verbs OXC backend — flag docstrings that open with
//! a banned verb on function-like declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

const BANNED_VERBS: &[&str] = &[
    "reads",
    "pulls",
    "fetches",
    "loads",
    "sums",
    "counts",
    "aggregates",
    "iterates",
];

fn first_word(body: &str) -> Option<String> {
    body.split_whitespace().next().map(|w| {
        w.trim_matches(|c: char| !c.is_ascii_alphabetic())
            .to_lowercase()
    })
}

fn strip_markers(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.lines() {
        let t = line
            .trim()
            .trim_start_matches("/**")
            .trim_start_matches("///")
            .trim_start_matches("//")
            .trim_start_matches("/*")
            .trim_start_matches('*')
            .trim_end_matches("*/")
            .trim();
        if !t.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(t);
        }
    }
    out
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Collect function-like declaration spans so we can match comments
        // that immediately precede them.
        let mut func_starts: Vec<u32> = Vec::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::Function(f) => {
                    func_starts.push(f.span.start);
                }
                AstKind::VariableDeclarator(decl) => {
                    // Only if the value is an arrow/function expression.
                    let is_fn = decl.init.as_ref().is_some_and(|init| {
                        matches!(
                            init,
                            oxc_ast::ast::Expression::ArrowFunctionExpression(_)
                                | oxc_ast::ast::Expression::FunctionExpression(_)
                        )
                    });
                    if is_fn {
                        // Use the start of the variable declaration statement, not
                        // the declarator, since the comment precedes `const/let/var`.
                        func_starts.push(decl.span.start);
                    }
                }
                _ => {}
            }
        }

        func_starts.sort_unstable();

        for comment in semantic.comments().iter() {
            let c_start = comment.span.start as usize;
            let c_end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(c_start..c_end) else {
                continue;
            };

            // Check if a function-like starts right after this comment.
            // Allow up to a small gap for whitespace/newlines.
            let after_comment = c_end as u32;
            let has_following_func = func_starts
                .binary_search(&after_comment)
                .is_ok()
                || func_starts.iter().any(|&fs| {
                    fs > after_comment
                        && fs < after_comment + 20
                        && ctx.source[c_end..fs as usize].trim().is_empty()
                });

            if !has_following_func {
                continue;
            }

            let body = strip_markers(raw);
            let Some(first) = first_word(&body) else {
                continue;
            };
            if !BANNED_VERBS.contains(&first.as_str()) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, c_start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Docstring opens with `{first}` \u{2014} start with intent, not implementation (e.g. `Return\u{2026}`, `Ensure\u{2026}`)."
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_reads_verb() {
        let src = "/** Reads the user from storage */\nfunction loadUser() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_iterates_verb() {
        let src = "// iterates over nodes\nfunction walk() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_intent_verb() {
        let src =
            "/** Return the current user, creating one if missing. */\nfunction loadUser() {}";
        assert!(run(src).is_empty());
    }
}
