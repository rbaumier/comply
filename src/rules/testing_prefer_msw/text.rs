use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const HTTP_CLIENT_MOCKS: &[&str] = &[
    "vi.mock('axios')", "vi.mock(\"axios\")",
    "vi.mock('node-fetch')", "vi.mock(\"node-fetch\")",
    "vi.mock('cross-fetch')", "vi.mock(\"cross-fetch\")",
    "global.fetch = vi.fn()", "globalThis.fetch = vi.fn()",
    "jest.spyOn(global, 'fetch')", "jest.spyOn(globalThis, 'fetch')",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if HTTP_CLIENT_MOCKS.iter().any(|m| t.contains(m)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "testing-prefer-msw".into(),
                    message: "Mocking the HTTP client directly is brittle — use MSW to intercept network requests at the handler level.".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn run_test(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), s))
    }
    fn run_src(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.ts"), s))
    }
    #[test]
    fn flags_axios_mock() { assert_eq!(run_test("vi.mock('axios')").len(), 1); }
    #[test]
    fn flags_global_fetch_mock() { assert_eq!(run_test("global.fetch = vi.fn()").len(), 1); }
    #[test]
    fn flags_node_fetch_mock() { assert_eq!(run_test("vi.mock(\"node-fetch\")").len(), 1); }
    #[test]
    fn ignores_non_test_files() { assert!(run_src("vi.mock('axios')").is_empty()); }
    #[test]
    fn allows_msw_handler() { assert!(run_test("server.use(http.get('/api', resolver))").is_empty()); }
}
