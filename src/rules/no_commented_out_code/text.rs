//! no-commented-out-code backend — heuristic detection of code in comments.
//!
//! Commented-out code is tech debt in disguise: it's unreviewable, unreachable,
//! and makes readers wonder whether it's intentionally disabled. Delete it —
//! git history keeps the original for you.
//!
//! Detection heuristic: a `//` comment line is flagged as commented-out code
//! if its trailing text contains at least two code-like signals (`;`, `=`,
//! `{`, `}`, `(`, `)`, `[`, `]`) AND starts with a lowercase letter, `const`,
//! `let`, `var`, `function`, `if`, `for`, `while`, `return`. Pure prose
//! comments usually don't match this shape.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const CODE_KEYWORDS: &[&str] = &[
    "const ",
    "let ",
    "var ",
    "function ",
    "if (",
    "if(",
    "for (",
    "for(",
    "while (",
    "while(",
    "return ",
    "return;",
    "await ",
    "throw ",
    "import ",
    "export ",
];

pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let Some(comment_body) = extract_line_comment(line) else {
                continue;
            };
            if looks_like_code(comment_body) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-commented-out-code".into(),
                    message: "This comment looks like commented-out code — \
                              delete it. Git history preserves the original."
                        .into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

/// Extract the body of a `//` comment on the given line, if any.
/// Skips `/* */` and doc comments — they're usually prose.
fn extract_line_comment(line: &str) -> Option<&str> {
    let comment_start = line.find("//")?;
    // Skip JSDoc / triple-slash — those are intentional.
    if line[comment_start..].starts_with("///") {
        return None;
    }
    Some(line[comment_start + 2..].trim())
}

/// Heuristic: comment body starts with a code keyword AND contains enough
/// punctuation to look like syntax.
fn looks_like_code(body: &str) -> bool {
    if body.is_empty() {
        return false;
    }
    let starts_with_keyword = CODE_KEYWORDS.iter().any(|kw| body.starts_with(kw));
    if !starts_with_keyword {
        return false;
    }
    let punct_count = body
        .bytes()
        .filter(|b| matches!(b, b';' | b'=' | b'{' | b'}' | b'(' | b')'))
        .count();
    punct_count >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx {
            path: Path::new("t.ts"),
            source,
        })
    }

    #[test]
    fn flags_commented_const() {
        assert_eq!(run("// const x = 5;").len(), 1);
    }

    #[test]
    fn flags_commented_function_call() {
        assert_eq!(run("// return foo(bar);").len(), 1);
    }

    #[test]
    fn allows_prose_comment() {
        assert!(run("// This function computes the total cost.").is_empty());
    }

    #[test]
    fn allows_triple_slash_doc_comment() {
        assert!(run("/// Returns the parsed result.").is_empty());
    }

    #[test]
    fn allows_short_label_comment() {
        assert!(run("// setup").is_empty());
    }
}
