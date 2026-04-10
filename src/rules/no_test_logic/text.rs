use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const CONTROL_FLOW: &[&str] = &["if (", "if(", "for (", "for(", "while (", "while(", "switch (", "switch("];

const SETUP_HOOKS: &[&str] = &["beforeEach", "afterEach", "beforeAll", "afterAll"];

/// Returns the leading whitespace length (number of spaces/tabs).
fn indent_level(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Returns true if `trimmed` starts with a control-flow keyword followed by `(`.
fn has_control_flow(trimmed: &str) -> bool {
    CONTROL_FLOW.iter().any(|kw| trimmed.starts_with(kw))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        let mut in_test = false;
        let mut test_indent: usize = 0;
        let mut brace_depth: i32 = 0;
        let mut in_hook = false;
        let mut hook_indent: usize = 0;

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();

            // Detect setup/teardown hooks — don't flag control flow inside them.
            if !in_test {
                // hooks at top-level inside describe are fine
            }

            if in_test && SETUP_HOOKS.iter().any(|h| trimmed.starts_with(h)) {
                in_hook = true;
                hook_indent = indent_level(line);
            }

            // Detect test body entry: `it(` / `test(` / `it.each` / `test.each`
            if !in_test
                && (trimmed.starts_with("it(")
                    || trimmed.starts_with("it.each")
                    || trimmed.starts_with("test(")
                    || trimmed.starts_with("test.each"))
            {
                in_test = true;
                test_indent = indent_level(line);
                brace_depth = 0;
                in_hook = false;
            }

            if in_test {
                // Track brace depth relative to test entry.
                for ch in line.chars() {
                    match ch {
                        '{' => brace_depth += 1,
                        '}' => brace_depth -= 1,
                        _ => {}
                    }
                }

                // Check for hook exit.
                if in_hook && trimmed.starts_with("});") && indent_level(line) <= hook_indent {
                    in_hook = false;
                }

                // Flag control flow inside the test body (but not inside hooks).
                if !in_hook && brace_depth > 0 && has_control_flow(trimmed) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-test-logic".into(),
                        message: format!(
                            "Control-flow `{}` inside test body — tests should have a single linear assertion path.",
                            trimmed.split('(').next().unwrap_or("?"),
                        ),
                        severity: Severity::Warning,
                    });
                }

                // Exit test body when braces close back to zero.
                if brace_depth <= 0 {
                    in_test = false;
                    in_hook = false;
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

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_if_in_test() {
        let source = r#"
test('x', () => {
    if (true) {
        expect(1).toBe(1);
    }
});
"#;
        let diags = run("app/__tests__/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-test-logic");
        assert!(diags[0].message.contains("if"));
    }

    #[test]
    fn flags_for_in_test() {
        let source = r#"
it('does stuff', () => {
    for (const x of items) {
        expect(x).toBeDefined();
    }
});
"#;
        let diags = run("src/utils.spec.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("for"));
    }

    #[test]
    fn flags_while_in_test() {
        let source = r#"
test('loops', () => {
    while (condition) {
        doSomething();
    }
});
"#;
        let diags = run("src/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("while"));
    }

    #[test]
    fn flags_switch_in_test() {
        let source = r#"
test('switch', () => {
    switch (value) {
        case 1: break;
    }
});
"#;
        let diags = run("src/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("switch"));
    }

    #[test]
    fn allows_before_each() {
        let source = r#"
describe('suite', () => {
    beforeEach(() => {
        if (condition) {
            setup();
        }
    });

    test('x', () => {
        expect(1).toBe(1);
    });
});
"#;
        let diags = run("src/foo.test.ts", source);
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let source = r#"
if (condition) {
    doSomething();
}
"#;
        assert!(run("src/utils.ts", source).is_empty());
    }

    #[test]
    fn allows_test_each() {
        let source = r#"
test.each([1, 2, 3])('works with %i', (n) => {
    expect(n).toBeGreaterThan(0);
});
"#;
        let diags = run("src/foo.test.ts", source);
        assert!(diags.is_empty());
    }

    #[test]
    fn multiple_tests_independent() {
        let source = r#"
test('first', () => {
    if (x) { fail(); }
});

test('second', () => {
    expect(1).toBe(1);
});
"#;
        let diags = run("src/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }
}
