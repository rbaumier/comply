use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if a line contains `new Promise(` with a body that is a single
/// `resolve(...)` or `reject(...)` call — the classic anti-pattern.
fn is_promise_shorthand_candidate(line: &str) -> bool {
    if !line.contains("new Promise(") && !line.contains("new Promise<") {
        return false;
    }

    // Patterns we look for (all on a single line):
    //   new Promise((resolve) => resolve(...))
    //   new Promise((resolve) => { resolve(...) })
    //   new Promise((_, reject) => reject(...))
    //   new Promise((resolve, reject) => resolve(...))
    //   new Promise(function(resolve) { resolve(...) })
    let after_promise = if let Some(pos) = line.find("new Promise(") {
        &line[pos + 12..]
    } else if let Some(pos) = line.find("new Promise<") {
        // Skip generic type parameter: new Promise<Type>((resolve) => ...)
        let rest = &line[pos + 12..];
        if let Some(gt) = rest.find(">(") {
            &rest[gt + 2..]
        } else {
            return false;
        }
    } else {
        return false;
    };

    // The entire promise body must be on this one line for us to flag it.
    // Check for arrow-style or function-style single resolve/reject.
    let body = after_promise.trim();

    // Arrow: `(resolve) => resolve(...))`
    // Arrow: `(_, reject) => reject(...))`
    if body.contains("=> resolve(") || body.contains("=> reject(") {
        return true;
    }
    // Function style: `function(resolve) { resolve(...) }`
    if body.contains("{ resolve(") || body.contains("{ reject(") {
        return true;
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_promise_shorthand_candidate(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-promise-shorthand".into(),
                    message: "`new Promise` wrapping a single resolve/reject — use `Promise.resolve()`/`Promise.reject()` instead.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_promise_resolve_shorthand() {
        let src = r#"const p = new Promise((resolve) => resolve(42));"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-promise-shorthand");
    }

    #[test]
    fn flags_promise_reject_shorthand() {
        let src = r#"const p = new Promise((_, reject) => reject(new Error("fail")));"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_promise_with_logic() {
        let src = r#"const p = new Promise((resolve, reject) => {
  fetchData().then(resolve).catch(reject);
});"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_resolve_static() {
        let src = r#"const p = Promise.resolve(42);"#;
        assert!(run(src).is_empty());
    }
}
