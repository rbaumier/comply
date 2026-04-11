use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Normalize whitespace in a block body for comparison purposes.
fn normalize(body: &str) -> String {
    body.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Given lines and a starting index pointing to a line that contains `{`,
/// find the content between the LAST `{` on that line and its matching `}`.
/// This handles lines like `} else {` where we want the block opened by the
/// trailing `{`, not the earlier `}`.
/// Returns `(inner_body, index_of_line_containing_closing_brace)`.
fn extract_block(lines: &[&str], open_line: usize) -> Option<(String, usize)> {
    let line = lines[open_line];

    // Find the last `{` on the opening line — that's our block opener.
    // Everything before it (including any `}` from a prior block) is ignored.
    if !line.contains('{') {
        return None;
    }

    // Count only braces AFTER the last `{` on the opening line to get
    // the initial depth. The last `{` contributes depth=1.
    let last_open = line.rfind('{')?;
    let mut depth: i32 = 1;
    for ch in line[last_open + 1..].chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    // Block opened and closed on same line — empty body
                    return Some((String::new(), open_line));
                }
            }
            _ => {}
        }
    }

    // Scan subsequent lines
    let mut body_lines: Vec<&str> = Vec::new();
    for (idx, line) in lines.iter().enumerate().skip(open_line + 1) {
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let inner = body_lines.join("\n");
                        return Some((inner, idx));
                    }
                }
                _ => {}
            }
        }
        if depth > 0 {
            body_lines.push(line);
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let trimmed = lines[i].trim();

            // Look for `if (...) {` pattern
            if !trimmed.starts_with("if ") && !trimmed.starts_with("if(") {
                i += 1;
                continue;
            }
            if !trimmed.contains('{') {
                i += 1;
                continue;
            }

            let if_start_line = i;
            let mut branches: Vec<String> = Vec::new();

            // Extract the if body
            let Some((if_body, close_idx)) = extract_block(&lines, i) else {
                i += 1;
                continue;
            };
            branches.push(normalize(&if_body));

            // Now look for continuations starting from the close line
            i = close_idx;

            loop {
                if i >= lines.len() {
                    break;
                }

                let close_line = lines[i].trim();

                // Check if this line or the next has `else if` or `else`
                let (cont_type, cont_line) =
                    if close_line.contains("else") && close_line.contains('{') {
                        if close_line.contains("else if") {
                            ("else_if", i)
                        } else {
                            ("else", i)
                        }
                    } else if i + 1 < lines.len() {
                        let next = lines[i + 1].trim();
                        if next.starts_with("else if") && next.contains('{') {
                            ("else_if", i + 1)
                        } else if next.starts_with("else") && next.contains('{') {
                            ("else", i + 1)
                        } else {
                            ("none", 0)
                        }
                    } else {
                        ("none", 0)
                    };

                match cont_type {
                    "else_if" => {
                        let Some((body, close)) = extract_block(&lines, cont_line) else {
                            break;
                        };
                        branches.push(normalize(&body));
                        i = close;
                    }
                    "else" => {
                        let Some((body, close)) = extract_block(&lines, cont_line) else {
                            break;
                        };
                        branches.push(normalize(&body));
                        i = close + 1;
                        break; // `else` is always the last branch
                    }
                    _ => {
                        i += 1;
                        break;
                    }
                }
            }

            // Need at least 2 branches (if + else) to be meaningful
            if branches.len() >= 2 && !branches[0].is_empty() {
                let all_same = branches.iter().all(|b| *b == branches[0]);
                if all_same {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: if_start_line + 1,
                        column: 1,
                        rule_id: "no-all-duplicated-branches".into(),
                        message: format!(
                            "All {} branches have identical code — the conditional is pointless.",
                            branches.len()
                        ),
                        severity: Severity::Error,
                    });
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
    fn flags_identical_if_else() {
        let source = r#"
if (condition) {
    doSomething();
} else {
    doSomething();
}
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("2 branches"));
    }

    #[test]
    fn flags_identical_if_else_if_else() {
        let source = r#"
if (a) {
    doSomething();
} else if (b) {
    doSomething();
} else {
    doSomething();
}
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("3 branches"));
    }

    #[test]
    fn allows_different_branches() {
        let source = r#"
if (condition) {
    doA();
} else {
    doB();
}
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_if_without_else() {
        let source = r#"
if (condition) {
    doSomething();
}
"#;
        // Only one branch — no comparison possible
        assert!(run(source).is_empty());
    }
}
