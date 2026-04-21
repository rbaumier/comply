use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            let is_test_call = t.starts_with("test(") || t.starts_with("it(")
                || t.contains("  test(") || t.contains("  it(");
            if !is_test_call { continue; }
            if let Some(name) = extract_test_name(t)
                && name.contains(" and ")
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "testing-no-and-in-test-name".into(),
                    message: format!("Test name {:?} contains \" and \" — split into two focused tests.", name),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

fn extract_test_name(line: &str) -> Option<String> {
    for prefix in &["test('", "it('", "test(\"", "it(\""] {
        if let Some(pos) = line.find(prefix) {
            let rest = &line[pos + prefix.len()..];
            let close = if prefix.ends_with('\'') { '\'' } else { '"' };
            if let Some(end) = rest.find(close) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), s))
    }
    #[test]
    fn flags_and_in_name() {
        assert_eq!(run("test('validates email and sends confirmation', () => {})").len(), 1);
    }
    #[test]
    fn flags_it_with_and() {
        assert_eq!(run("it('creates user and returns token', () => {})").len(), 1);
    }
    #[test]
    fn allows_single_behavior() {
        assert!(run("test('validates email format', () => {})").is_empty());
    }
    #[test]
    fn allows_and_in_describe() {
        assert!(run("describe('login and registration', () => {})").is_empty());
    }
}
