//! no-mock-fetch-directly backend — detect direct mocking of HTTP clients
//! in test files.
//!
//! Flags `vi.mock('axios')`, `jest.mock('node-fetch')`,
//! `global.fetch = vi.fn()`, `globalThis.fetch = jest.fn()`, and similar
//! patterns. The fix is to use MSW instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// Module names that should not be mocked directly.
const MOCKED_MODULES: &[&str] = &["axios", "node-fetch"];

/// Prefixes for global fetch reassignment.
const FETCH_GLOBALS: &[&str] = &["global.fetch", "globalThis.fetch"];

/// Mock factory calls from test frameworks.
const MOCK_FNS: &[&str] = &["vi.fn()", "jest.fn()"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(message) = detect_mock(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-mock-fetch-directly".into(),
                    message,
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

/// Returns `true` when the file path looks like a test file.
fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Returns a diagnostic message if `line` contains a banned mock pattern.
fn detect_mock(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Check for vi.mock('axios') / jest.mock("node-fetch") etc.
    for framework in &["vi", "jest"] {
        for module in MOCKED_MODULES {
            let single = format!("{framework}.mock('{module}')");
            let double = format!("{framework}.mock(\"{module}\")");
            if trimmed.contains(&single) || trimmed.contains(&double) {
                return Some(format!(
                    "Direct mock of `{module}` via `{framework}.mock` — \
                     use MSW to intercept at the network level instead."
                ));
            }
        }
    }

    // Check for global.fetch = vi.fn() / globalThis.fetch = jest.fn() etc.
    for global in FETCH_GLOBALS {
        if trimmed.contains(global) {
            for mock_fn in MOCK_FNS {
                if trimmed.contains(mock_fn) {
                    return Some(format!(
                        "Reassigning `{global}` with `{mock_fn}` — \
                         use MSW to intercept at the network level instead."
                    ));
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_vi_mock_axios() {
        let diags = run("src/api.test.ts", "vi.mock('axios')");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("axios"));
    }

    #[test]
    fn flags_jest_mock_axios_double_quotes() {
        let diags = run("src/api.spec.ts", "jest.mock(\"axios\")");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_vi_mock_node_fetch() {
        let diags = run("src/__tests__/http.ts", "vi.mock('node-fetch')");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("node-fetch"));
    }

    #[test]
    fn flags_global_fetch_vi_fn() {
        let diags = run("src/api.test.ts", "global.fetch = vi.fn()");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("global.fetch"));
    }

    #[test]
    fn flags_global_this_fetch_jest_fn() {
        let diags = run("src/api_test.ts", "globalThis.fetch = jest.fn()");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("globalThis.fetch"));
    }

    #[test]
    fn allows_msw_import() {
        let diags = run("src/api.test.ts", "import { setupServer } from 'msw/node'");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let diags = run("src/api.ts", "vi.mock('axios')");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_clean_test_file() {
        let diags = run("src/api.test.ts", "import { rest } from 'msw'");
        assert!(diags.is_empty());
    }
}
