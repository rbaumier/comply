use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects the pattern:
/// ```js
/// if (condition) el.classList.add('x') else el.classList.remove('x')
/// ```
/// or
/// ```js
/// condition ? el.classList.add('x') : el.classList.remove('x')
/// ```
///
/// These should be replaced with `el.classList.toggle('x', condition)`.
fn has_classlist_add_remove_pair(line: &str) -> bool {
    // Pattern 1: ternary — `? ...classList.add(` ... `: ...classList.remove(`
    // (or the reverse: add in the else branch)
    if line.contains("classList.add(") && line.contains("classList.remove(") {
        return true;
    }
    false
}

/// Detects inline `el.classList[cond ? 'add' : 'remove']('x')` pattern.
fn has_classlist_computed_add_remove(line: &str) -> bool {
    if !line.contains("classList[") {
        return false;
    }
    // Look for classList[ ... 'add' ... 'remove' ... ] or classList[ ... "add" ... "remove" ... ]
    if let Some(start) = line.find("classList[") {
        let rest = &line[start..];
        let has_add = rest.contains("'add'") || rest.contains("\"add\"");
        let has_remove = rest.contains("'remove'") || rest.contains("\"remove\"");
        return has_add && has_remove;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }

            // Single-line ternary or single-line if/else with both add and remove
            if has_classlist_add_remove_pair(trimmed) || has_classlist_computed_add_remove(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-classlist-toggle".into(),
                    message: "Prefer `classList.toggle('class', condition)` over conditional `classList.add/remove`.".into(),
                    severity: Severity::Warning,
                });
                continue;
            }

            // Multi-line: if-block with classList.add followed by else-block with classList.remove (or vice versa)
            if trimmed.contains("classList.add(") || trimmed.contains("classList.remove(") {
                let has_add = trimmed.contains("classList.add(");
                let target = if has_add {
                    "classList.remove("
                } else {
                    "classList.add("
                };
                // Look ahead up to 4 lines for the counterpart
                for lookahead in 1..=4 {
                    if idx + lookahead >= lines.len() {
                        break;
                    }
                    let next_trimmed = lines[idx + lookahead].trim();
                    if next_trimmed.contains(target) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "prefer-classlist-toggle".into(),
                            message: "Prefer `classList.toggle('class', condition)` over conditional `classList.add/remove`.".into(),
                            severity: Severity::Warning,
                        });
                        break;
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
    fn flags_ternary_classlist() {
        let code = "cond ? el.classList.add('active') : el.classList.remove('active');";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_if_else_classlist() {
        let code = r#"if (isActive) {
  el.classList.add('active');
} else {
  el.classList.remove('active');
}"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_computed_classlist() {
        let code = "el.classList[cond ? 'add' : 'remove']('active');";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_toggle() {
        assert!(run("el.classList.toggle('active', cond);").is_empty());
    }

    #[test]
    fn allows_standalone_add() {
        assert!(run("el.classList.add('active');").is_empty());
    }
}
