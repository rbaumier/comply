use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if a line ends with a control-flow statement without an opening brace.
/// Matches: `if (...)`, `for (...)`, `while (...)` where the line does NOT end with `{`.
fn is_braceless_control_flow(trimmed: &str) -> bool {
    let stripped = trimmed.trim_end();

    // Must not end with `{` or `;` (single-line body on same line)
    if stripped.ends_with('{') || stripped.ends_with(';') {
        return false;
    }

    // Must not end with `)` that is followed by a single-line statement on same line
    // after closing paren. We only care about lines that end with `)`.
    if !stripped.ends_with(')') {
        return false;
    }

    // Check for control-flow keyword at start (ignoring `} else`)
    let check_start = if stripped.starts_with("} else ") || stripped.starts_with("}else ") {
        stripped
            .find("else ")
            .map(|p| &stripped[p + 5..])
            .unwrap_or("")
    } else {
        stripped
    };

    for keyword in &["if ", "if(", "for ", "for(", "while ", "while("] {
        if check_start.starts_with(keyword) {
            return true;
        }
    }

    // Handle `else if` specifically
    if stripped.starts_with("else if") || stripped.starts_with("} else if") {
        return true;
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if is_braceless_control_flow(trimmed) {
                // Check if the next non-empty line exists and is a statement (multiline body)
                if let Some(next_idx) =
                    (idx + 1..lines.len()).find(|&i| !lines[i].trim().is_empty())
                {
                    let next_trimmed = lines[next_idx].trim();

                    // The next line is a statement (not a `{`)
                    if !next_trimmed.starts_with('{') {
                        // Check if there's yet another statement line after — meaning body spans multiple lines
                        // OR just the fact that the body is on a separate line from the condition is enough
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "no-unenclosed-multiline-block".into(),
                            message: format!(
                                "`{}` body is on the next line without curly braces — wrap it in `{{}}`.",
                                if trimmed.contains("while") { "while" }
                                else if trimmed.contains("for") { "for" }
                                else { "if" }
                            ),
                            severity: Severity::Error,
                        });
                    }
                }
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_multiline_if_without_braces() {
        let src = r#"
if (condition)
    doSomething();
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiline_for_without_braces() {
        let src = r#"
for (const x of items)
    process(x);
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_line_if() {
        let src = "if (condition) doSomething();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_braced_if() {
        let src = r#"
if (condition) {
    doSomething();
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_while_without_braces() {
        let src = r#"
while (running)
    tick();
"#;
        assert_eq!(run(src).len(), 1);
    }
}
