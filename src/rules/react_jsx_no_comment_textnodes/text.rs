//! react-jsx-no-comment-textnodes text backend.
//!
//! Detects `// comment` or `/* comment */` as text children inside JSX.
//! These render as visible text in the DOM instead of being actual comments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // A comment-like text node: line is just `// ...` or `/* ... */`
            let is_line_comment = trimmed.starts_with("//") && !trimmed.starts_with("///");
            let is_block_comment =
                trimmed.starts_with("/*") && trimmed.ends_with("*/");

            if !is_line_comment && !is_block_comment {
                continue;
            }

            // Check context: previous line should end with `>` or `}`
            // (closing of a JSX open tag) and next line should start with
            // `<` or `{` (a JSX child).
            let prev_looks_jsx = idx > 0 && {
                let prev = lines[idx - 1].trim();
                prev.ends_with('>') || prev.ends_with('}')
            };
            let next_looks_jsx = idx + 1 < lines.len() && {
                let next = lines[idx + 1].trim();
                next.starts_with('<') || next.starts_with('{')
            };

            if prev_looks_jsx && next_looks_jsx {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "react-jsx-no-comment-textnodes".into(),
                    message: "Comment as JSX text child will be rendered as \
                              visible text. Use `{/* comment */}` instead."
                        .into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_line_comment_in_jsx() {
        let src = r#"
function App() {
    return (
        <div>
            // this is a comment
            <span>hello</span>
        </div>
    );
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_block_comment_in_jsx() {
        let src = r#"
function App() {
    return (
        <div>
            /* this is a comment */
            <span>hello</span>
        </div>
    );
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_proper_jsx_comment() {
        let src = r#"
function App() {
    return (
        <div>
            {/* this is a proper comment */}
            <span>hello</span>
        </div>
    );
}
"#;
        assert!(run(src).is_empty());
    }
}
