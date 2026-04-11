//! no-test-logic backend — reject control-flow logic inside test bodies.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    if !is_test_file(ctx.path) {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    let mut in_test = false;
    let mut _test_indent: usize = 0;
    let mut brace_depth: i32 = 0;
    let mut in_hook = false;
    let mut hook_indent: usize = 0;

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();

        if in_test && SETUP_HOOKS.iter().any(|h| trimmed.starts_with(h)) {
            in_hook = true;
            hook_indent = indent_level(line);
        }

        // Detect test body entry
        if !in_test
            && (trimmed.starts_with("it(")
                || trimmed.starts_with("it.each")
                || trimmed.starts_with("test(")
                || trimmed.starts_with("test.each"))
        {
            in_test = true;
            _test_indent = indent_level(line);
            brace_depth = 0;
            in_hook = false;
        }

        if in_test {
            for ch in line.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }

            if in_hook && trimmed.starts_with("});") && indent_level(line) <= hook_indent {
                in_hook = false;
            }

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

            if brace_depth <= 0 {
                in_test = false;
                in_hook = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run_test_file(path: &str, source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    #[test]
    fn flags_if_in_test() {
        let source = "test('x', () => {\n    if (true) {\n        expect(1).toBe(1);\n    }\n});";
        let diags = run_test_file("app/__tests__/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("if"));
    }

    #[test]
    fn flags_for_in_test() {
        let source = "it('does stuff', () => {\n    for (const x of items) {\n        expect(x).toBeDefined();\n    }\n});";
        let diags = run_test_file("src/utils.spec.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("for"));
    }

    #[test]
    fn ignores_non_test_file() {
        let source = "if (condition) {\n    doSomething();\n}";
        assert!(run_test_file("src/utils.ts", source).is_empty());
    }
}
