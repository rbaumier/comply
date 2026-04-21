use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

fn extract_test_name(line: &str) -> Option<String> {
    for prefix in &["test('", "it('", "test(\"", "it(\""] {
        if let Some(pos) = line.find(prefix) {
            let rest = &line[pos + prefix.len()..];
            let close = if prefix.ends_with('\'') { '\'' } else { '"' };
            if let Some(end) = rest.find(close) {
                return Some(rest[..end].to_lowercase());
            }
        }
    }
    None
}

fn common_prefix(a: &str, b: &str) -> String {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).map(|(c, _)| c).collect()
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return Vec::new();
        }
        let mut test_names: Vec<(usize, String)> = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(name) = extract_test_name(line) {
                test_names.push((idx + 1, name));
            }
        }
        if test_names.len() < 3 { return Vec::new(); }

        let mut flagged: HashSet<usize> = HashSet::new();
        let mut diagnostics = Vec::new();
        let n = test_names.len();
        for i in 0..n {
            if flagged.contains(&i) { continue; }
            for j in i + 1..n {
                let prefix = common_prefix(&test_names[i].1, &test_names[j].1);
                if prefix.len() < 8 { continue; }
                for k in j + 1..n {
                    if test_names[k].1.starts_with(&prefix) && !flagged.contains(&i) {
                        flagged.insert(i);
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: test_names[i].0,
                            column: 1,
                            rule_id: "testing-prefer-test-each".into(),
                            message: format!(
                                "3+ tests share the prefix {:?} — use `test.each` to express these as a data-driven table.",
                                prefix.trim()
                            ),
                            severity: Severity::Warning,
                            span: None,
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
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), s))
    }
    #[test]
    fn flags_three_tests_with_common_prefix() {
        let src = [
            "test('returns 200 for valid input', () => {})",
            "test('returns 400 for missing field', () => {})",
            "test('returns 422 for invalid format', () => {})",
        ].join("\n");
        assert!(!run(&src).is_empty());
    }
    #[test]
    fn allows_two_tests_only() {
        let src = [
            "test('returns 200 for valid', () => {})",
            "test('returns 400 for invalid', () => {})",
        ].join("\n");
        assert!(run(&src).is_empty());
    }
    #[test]
    fn allows_tests_with_short_or_no_common_prefix() {
        let src = [
            "test('creates a user', () => {})",
            "test('deletes a post', () => {})",
            "test('updates a comment', () => {})",
        ].join("\n");
        assert!(run(&src).is_empty());
    }
}
