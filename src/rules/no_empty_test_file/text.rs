use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
}

const TEST_MARKERS: &[&str] = &["test(", "it(", "describe(", "expect(", "assert(", "assert."];

fn has_test_content(source: &str) -> bool {
    for marker in TEST_MARKERS {
        if source.contains(marker) {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }
        if has_test_content(ctx.source) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "no-empty-test-file".into(),
            message:
                "Test file contains no test assertions (`test(`, `it(`, `describe(`, `expect(`)."
                    .into(),
            severity: Severity::Error,
            span: None,
        }]
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
    fn flags_empty_test_file() {
        assert_eq!(
            run("utils.test.ts", "import { foo } from './foo';").len(),
            1
        );
    }

    #[test]
    fn flags_empty_spec_file() {
        assert_eq!(run("utils.spec.ts", "// TODO: add tests").len(), 1); // comply-ignore: todo-needs-issue-link — test content, not a real marker.
    }

    #[test]
    fn flags_tests_dir() {
        assert_eq!(
            run("__tests__/utils.ts", "export const helper = true;").len(),
            1
        );
    }

    #[test]
    fn allows_test_file_with_tests() {
        assert!(
            run(
                "utils.test.ts",
                "test('adds 1+1', () => { expect(1+1).toBe(2); });"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_non_test_file() {
        assert!(run("utils.ts", "export const foo = 1;").is_empty());
    }

    #[test]
    fn allows_assert_style_tests() {
        assert!(
            run(
                "plugin.test.js",
                "import assert from 'assert';\nassert.equal(result, expected);"
            )
            .is_empty()
        );
    }
}
