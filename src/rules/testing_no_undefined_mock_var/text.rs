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
        if !ctx.source.contains("vi.mock(") { return Vec::new(); }

        let mut module_lets: Vec<String> = Vec::new();
        let mut in_hoisted = false;
        let mut hoisted_depth: usize = 0;
        for line in ctx.source.lines() {
            let t = line.trim();
            if t.contains("vi.hoisted(") {
                in_hoisted = true;
            }
            if in_hoisted {
                hoisted_depth = hoisted_depth
                    .saturating_add(t.matches('(').count())
                    .saturating_sub(t.matches(')').count());
                if hoisted_depth == 0 { in_hoisted = false; }
                continue;
            }
            if let Some(rest) = line.strip_prefix("let ") {
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !name.is_empty() { module_lets.push(name); }
            }
        }
        if module_lets.is_empty() { return Vec::new(); }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if line.trim().contains("vi.mock(") {
                let mock_line = i + 1;
                let mut body = String::new();
                let mut depth: usize = 0;
                let mut j = i;
                while j < lines.len() {
                    let l = lines[j];
                    depth = depth
                        .saturating_add(l.matches('(').count())
                        .saturating_sub(l.matches(')').count());
                    body.push_str(l);
                    body.push('\n');
                    if depth == 0 && j > i { break; }
                    j += 1;
                }
                for var_name in &module_lets {
                    if body.contains(var_name.as_str()) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: mock_line,
                            column: 1,
                            rule_id: "testing-no-undefined-mock-var".into(),
                            message: format!(
                                "`{}` is declared at module level and referenced in a `vi.mock()` factory — it will be `undefined` due to hoisting. Declare it inside `vi.hoisted()` instead.",
                                var_name
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        break;
                    }
                }
                i = j;
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
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), s))
    }
    #[test]
    fn flags_module_let_in_mock_factory() {
        let src = r#"
let mockFn = vi.fn()
vi.mock('module', () => ({ default: mockFn }))
"#;
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_var_inside_hoisted() {
        let src = r#"
const mockFn = vi.hoisted(() => vi.fn())
vi.mock('module', () => ({ default: mockFn }))
"#;
        assert!(run(src).is_empty());
    }
    #[test]
    fn allows_mock_without_module_lets() {
        let src = r#"
vi.mock('module', () => ({ default: vi.fn() }))
"#;
        assert!(run(src).is_empty());
    }
    #[test]
    fn ignores_non_test_files() {
        let src = "let x = 1\nvi.mock('m', () => ({ a: x }))";
        let d = Check.check(&CheckCtx::for_test(Path::new("foo.ts"), src));
        assert!(d.is_empty());
    }
}
