use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    // Files under a `__fixtures__/` directory are input data (e.g. Babel transform
    // input fixtures), not test specs, even when nested inside `__tests__/`.
    if path.components().any(|c| c.as_os_str() == "__fixtures__") {
        return false;
    }
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
    drives_imported_runner(source)
}

/// True when the file imports a symbol and then invokes it as a top-level
/// statement, e.g. `import { testTokenization } from '...'; testTokenization(...)`.
///
/// This captures project-local custom test runners that wrap `it`/`test`
/// internally, so the file carries real test logic without any standard marker.
/// A genuinely empty test file (lone import, comment, or bare declaration) has
/// no such top-level invocation and is still flagged.
fn drives_imported_runner(source: &str) -> bool {
    let imported = imported_bindings(source);
    if imported.is_empty() {
        return false;
    }
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        imported.iter().any(|name| is_call_statement(trimmed, name))
    })
}

/// `true` when `line` begins with `name(`, i.e. `name` is being called at the
/// start of a statement rather than referenced as part of a larger identifier.
fn is_call_statement(line: &str, name: &str) -> bool {
    let Some(rest) = line.strip_prefix(name) else {
        return false;
    };
    rest.starts_with('(')
}

/// Collects the binding names introduced by `import` statements: default,
/// namespace (`* as ns`), and named bindings (honoring `as` aliases).
fn imported_bindings(source: &str) -> Vec<String> {
    let mut bindings = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(after_import) = trimmed.strip_prefix("import ") else {
            continue;
        };
        // Side-effect import (`import './x'`) has no bindings.
        let Some(clause) = after_import.split(" from ").next() else {
            continue;
        };
        collect_clause_bindings(clause, &mut bindings);
    }
    bindings
}

fn collect_clause_bindings(clause: &str, bindings: &mut Vec<String>) {
    if let Some(open) = clause.find('{') {
        let head = &clause[..open];
        for name in head.split(',') {
            push_binding(name, bindings);
        }
        if let Some(close) = clause[open + 1..].find('}') {
            let inner = &clause[open + 1..open + 1 + close];
            for spec in inner.split(',') {
                let alias = spec.split(" as ").last().unwrap_or(spec);
                push_binding(alias, bindings);
            }
        }
    } else {
        for name in clause.split(',') {
            push_binding(name, bindings);
        }
    }
}

/// Normalizes a raw import token (default name or `* as ns`) and records it if
/// it is a plain identifier.
fn push_binding(raw: &str, bindings: &mut Vec<String>) {
    let token = raw.trim();
    let name = token.rsplit(" as ").next().unwrap_or(token).trim();
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
        bindings.push(name.to_string());
    }
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
    fn allows_fixtures_input_file() {
        assert!(
            run(
                "packages/babel-plugin/__tests__/__fixtures__/dynamic.js",
                "const x = <div/>"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_nested_fixtures_input_file() {
        assert!(run("__tests__/dynamic/__fixtures__/foo.js", "import './bar';").is_empty());
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

    #[test]
    fn allows_custom_imported_runner() {
        let source = "import { testTokenization } from '../test/testRunner';\n\
                      \n\
                      testTokenization('m3', [\n\
                      \t[{ line: '(**)', tokens: [{ startIndex: 0, type: 'comment.m3' }] }]\n\
                      ]);\n";
        assert!(run("src/languages/definitions/m3/m3.test.ts", source).is_empty());
    }

    #[test]
    fn allows_aliased_imported_runner() {
        let source = "import { testTokenization as runTokens } from './testRunner';\n\
                      runTokens('lang', []);\n";
        assert!(run("lang.test.ts", source).is_empty());
    }

    #[test]
    fn flags_imported_but_uncalled_binding() {
        assert_eq!(
            run("utils.test.ts", "import { helper } from './helper';").len(),
            1
        );
    }
}
