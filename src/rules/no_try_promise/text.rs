use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Patterns that typically return a promise.
const PROMISE_PATTERNS: &[&str] = &[
    ".then(",
    "fetch(",
    "axios(",
    "axios.get(",
    "axios.post(",
    "axios.put(",
    "axios.delete(",
    "axios.patch(",
];

/// Returns true if the line contains a promise-returning call without `await`.
fn has_unawaited_promise(line: &str) -> bool {
    if line.contains("await ") || line.contains("await(") {
        return false;
    }
    PROMISE_PATTERNS.iter().any(|p| line.contains(p))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut in_try = false;
        let mut brace_depth: i32 = 0;
        let mut try_brace_depth: i32 = 0;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Detect `try {`
            if !in_try && trimmed.starts_with("try ") && trimmed.contains('{') {
                in_try = true;
                // Count braces on this line to set try_brace_depth
                try_brace_depth = brace_depth;
                brace_depth += line.matches('{').count() as i32;
                brace_depth -= line.matches('}').count() as i32;
                continue;
            }

            if in_try {
                brace_depth += line.matches('{').count() as i32;
                brace_depth -= line.matches('}').count() as i32;

                if has_unawaited_promise(line) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-try-promise".into(),
                        message: "Promise inside try/catch without `await` — rejection won't be caught.".into(),
                        severity: Severity::Error,
                    });
                }

                if brace_depth <= try_brace_depth {
                    in_try = false;
                }
            } else {
                brace_depth += line.matches('{').count() as i32;
                brace_depth -= line.matches('}').count() as i32;
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
    fn flags_fetch_without_await_in_try() {
        let src = r#"
try {
    const res = fetch("/api");
} catch (e) {}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_then_without_await_in_try() {
        let src = r#"
try {
    getData().then(r => r.json());
} catch (e) {}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_awaited_fetch_in_try() {
        let src = r#"
try {
    const res = await fetch("/api");
} catch (e) {}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_fetch_outside_try() {
        let src = r#"const res = fetch("/api");"#;
        assert!(run(src).is_empty());
    }
}
