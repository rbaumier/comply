use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if a `case` clause uses comma-separated values or `||`.
fn has_bad_case(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with("case ") {
        return false;
    }
    // Extract the part after `case ` and before `:`
    let after_case = &trimmed[5..];
    if let Some(colon_pos) = after_case.find(':') {
        let clause = &after_case[..colon_pos];
        // Check for comma or logical OR
        if clause.contains(" , ") || clause.contains(", ") || clause.contains(" ,") {
            return true;
        }
        if clause.contains("||") {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_bad_case(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "comma-or-logical-or-case".into(),
                    message: "Switch `case` uses comma or `||` — use separate `case` clauses with fall-through instead.".into(),
                    severity: Severity::Error,
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_comma_in_case() {
        assert_eq!(run("    case 1, 2:").len(), 1);
    }

    #[test]
    fn flags_logical_or_in_case() {
        assert_eq!(run("    case 1 || 2:").len(), 1);
    }

    #[test]
    fn allows_simple_case() {
        assert!(run("    case 1:").is_empty());
    }

    #[test]
    fn allows_fallthrough_pattern() {
        let src = "    case 1:\n    case 2:";
        assert!(run(src).is_empty());
    }
}
