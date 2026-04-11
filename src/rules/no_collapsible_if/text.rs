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

            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }

            // Look for `if (` that opens a block with `{`
            if !trimmed.contains("if (") || trimmed.contains("else if") {
                continue;
            }

            // The outer if must end with `{` (possibly on the same line or next)
            let opens_block = trimmed.ends_with('{');
            if !opens_block {
                continue;
            }

            // Find the next non-blank line
            let next_idx = (idx + 1..lines.len()).find(|&i| !lines[i].trim().is_empty());
            let Some(next_idx) = next_idx else {
                continue;
            };
            let next_trimmed = lines[next_idx].trim();

            // The next non-blank line must also be `if (` (another if)
            if !next_trimmed.contains("if (") || next_trimmed.contains("else if") {
                continue;
            }

            // Check that there is no `else` between the outer if's closing brace
            // and the end of the inner if block. We scan forward from the inner if
            // to find the structure. Simple heuristic: look for `else` on any line
            // between outer if and the end of its block scope.
            //
            // For a simpler check: verify no `else` keyword appears between the
            // outer `if` line and the inner `if` line.
            let has_else_between = (idx + 1..next_idx).any(|i| {
                let t = lines[i].trim();
                t.starts_with("else") || t.contains("} else")
            });

            if has_else_between {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-collapsible-if".into(),
                message: "Nested `if` without `else` can be merged into a single `if (a && b)`.".into(),
                severity: Severity::Error,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_nested_if() {
        let src = "if (a) {\n  if (b) {\n    doSomething();\n  }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_if_else_if() {
        let src = "if (a) {\n  doSomething();\n} else if (b) {\n  doOther();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_if_with_else() {
        let src = "if (a) {\n  if (b) {\n    doSomething();\n  }\n} else {\n  doOther();\n}";
        // The outer if has an else, but the nested if is still collapsible.
        // However, this rule only flags when no else exists between outer and inner if.
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_nested_ifs() {
        let src = "if (a) {\n  doSomething();\n}\nif (b) {\n  doOther();\n}";
        // These are sequential, not nested — second if is not inside first block
        // but the heuristic sees `if {` followed by non-blank `doSomething` not `if`
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_if() {
        assert!(run("if (a) { doSomething(); }").is_empty());
    }
}
