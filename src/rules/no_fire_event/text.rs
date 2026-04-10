use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("fireEvent.") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-fire-event".into(),
                    message: "Prefer `userEvent` over `fireEvent` — `fireEvent` dispatches a single synthetic event and skips intermediate browser events.".into(),
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

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_fire_event_in_test() {
        let diags = run("components/__tests__/button.test.tsx", "fireEvent.click(button)");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-fire-event");
    }

    #[test]
    fn allows_user_event() {
        assert!(run("components/__tests__/button.test.tsx", "userEvent.click(button)").is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run("components/button.tsx", "fireEvent.click(button)").is_empty());
    }
}
