use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract the loop variable name from a `for (let/var/const IDENT ...` header.
fn extract_for_var(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with("for") {
        return None;
    }
    let rest = trimmed.strip_prefix("for")?.trim_start();
    let rest = rest.strip_prefix('(')?.trim_start();
    // skip let/var/const
    let rest = rest
        .strip_prefix("let")
        .or_else(|| rest.strip_prefix("var"))
        .or_else(|| rest.strip_prefix("const"))?
        .trim_start();
    // ident is everything up to the first non-ident char
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if let Some(var_name) = extract_for_var(lines[i]) {
                // Track brace depth to find the loop body
                let mut depth: i32 = 0;
                let mut body_started = false;
                // Count braces on the for-line itself
                for ch in lines[i].chars() {
                    if ch == '{' {
                        depth += 1;
                        body_started = true;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                let mut j = i + 1;
                while j < lines.len() {
                    let line = lines[j];
                    for ch in line.chars() {
                        if ch == '{' {
                            depth += 1;
                            body_started = true;
                        } else if ch == '}' {
                            depth -= 1;
                        }
                    }
                    if body_started && depth <= 0 {
                        break;
                    }
                    // Check for reassignment: `var_name = ` but not `==`
                    let trimmed = line.trim();
                    if let Some(pos) = trimmed.find(var_name) {
                        let after_var = &trimmed[pos + var_name.len()..].trim_start();
                        if after_var.starts_with('=') && !after_var.starts_with("==") {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: j + 1,
                                column: 1,
                                rule_id: "no-loop-counter-reassign".into(),
                                message: format!(
                                    "Loop counter `{var_name}` is reassigned inside the loop body."
                                ),
                                severity: Severity::Error,
                            });
                        }
                    }
                    j += 1;
                }
            }
            i += 1;
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
    fn flags_counter_reassign() {
        let src = "for (let i = 0; i < n; i++) {\n  i = 5;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_counter_reassign_var() {
        let src = "for (var j = 0; j < 10; j++) {\n  j = 0;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_normal_loop() {
        let src = "for (let i = 0; i < n; i++) {\n  console.log(i);\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_different_var() {
        let src = "for (let i = 0; i < n; i++) {\n  x = 5;\n}";
        assert!(run(src).is_empty());
    }
}
