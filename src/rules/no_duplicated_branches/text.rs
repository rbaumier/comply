use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract brace-delimited block bodies from consecutive if / else-if / else branches.
/// Returns Vec<(start_line, body_text)> for each branch found.
fn extract_branches(source: &str) -> Vec<Vec<(usize, String)>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut chains: Vec<Vec<(usize, String)>> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Detect start of an if-chain
        if !trimmed.starts_with("if ") && !trimmed.starts_with("if(") {
            i += 1;
            continue;
        }

        let mut chain: Vec<(usize, String)> = Vec::new();
        let mut line_idx = i;

        loop {
            let t = lines[line_idx].trim();

            // Find the opening `{` on this line
            let has_open = t.contains('{');
            if !has_open {
                // single-line branch without braces — skip this chain
                break;
            }

            // Collect body lines until the matching `}`.
            // Skip any leading `}` that closes a previous branch — only start
            // counting depth from the first `{` we encounter.
            let mut depth = 0i32;
            let mut found_open_brace = false;
            let mut body_lines: Vec<&str> = Vec::new();
            let mut j = line_idx;
            let mut found_close = false;

            while j < lines.len() {
                for ch in lines[j].chars() {
                    match ch {
                        '{' => {
                            if !found_open_brace {
                                found_open_brace = true;
                                depth = 1;
                            } else {
                                depth += 1;
                            }
                        }
                        '}' if found_open_brace => {
                            depth -= 1;
                            if depth == 0 {
                                found_close = true;
                            }
                        }
                        _ => {}
                    }
                }
                // Don't include the opening/closing brace lines in the body comparison
                if j != line_idx && !found_close {
                    body_lines.push(lines[j]);
                } else if j != line_idx && found_close {
                    // The closing brace line — only include content before `}`
                    let before_close = lines[j].trim().trim_end_matches('}').trim_end_matches('{').trim();
                    if !before_close.is_empty() && before_close != "else" && !before_close.starts_with("} else") {
                        body_lines.push(lines[j]);
                    }
                }
                if found_close {
                    break;
                }
                j += 1;
            }

            if !found_close {
                break;
            }

            let body: String = body_lines
                .iter()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("\n");
            chain.push((line_idx + 1, body)); // 1-based

            // Check what follows the closing `}`
            let close_line = lines[j].trim();
            // Closing line might have `} else if (...) {` or `} else {`
            let after_close = close_line.trim_start_matches('}').trim();

            if after_close.starts_with("else if") || after_close.starts_with("else if(") {
                line_idx = j;
                continue;
            } else if after_close.starts_with("else") {
                line_idx = j;
                continue;
            }

            // Next line might be `else ...`
            if j + 1 < lines.len() {
                let next = lines[j + 1].trim();
                if next.starts_with("else if") || next.starts_with("else if(") || next.starts_with("else") || next == "else {" {
                    line_idx = j + 1;
                    continue;
                }
            }

            break;
        }

        if chain.len() >= 2 {
            chains.push(chain);
        }
        i = line_idx + 1;
    }

    chains
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let chains = extract_branches(ctx.source);

        for chain in &chains {
            // Compare every pair of branches in the chain
            for i in 0..chain.len() {
                for j in (i + 1)..chain.len() {
                    if !chain[i].1.is_empty() && chain[i].1 == chain[j].1 {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: chain[j].0,
                            column: 1,
                            rule_id: "no-duplicated-branches".into(),
                            message: "This branch has the same body as another branch — merge conditions or remove the duplicate.".into(),
                            severity: Severity::Warning,
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
    fn flags_duplicate_if_else() {
        let src = "\
if (a) {
  doSomething();
} else {
  doSomething();
}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_duplicate_in_else_if_chain() {
        let src = "\
if (a) {
  foo();
} else if (b) {
  bar();
} else if (c) {
  foo();
}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_different_branches() {
        let src = "\
if (a) {
  foo();
} else {
  bar();
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_branch() {
        let src = "\
if (a) {
  foo();
}";
        assert!(run(src).is_empty());
    }
}
