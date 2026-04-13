//! playwright-no-hooks — disallow setup and teardown hooks.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const HOOKS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let name = match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or(""),
        "member_expression" => {
            if let Some(prop) = callee.child_by_field_name("property") {
                prop.utf8_text(source).unwrap_or("")
            } else {
                return;
            }
        }
        _ => return,
    };

    if !HOOKS.contains(&name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-hooks".into(),
        message: format!("Unexpected '{name}' hook."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, "app.test.ts")
    }

    #[test]
    fn flags_before_each() {
        let d = run_ts("beforeEach(() => { setup(); });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-hooks");
    }

    #[test]
    fn flags_after_all() {
        let d = run_ts("afterAll(() => { cleanup(); });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_hook() {
        let d = run_ts("test('works', () => { expect(1).toBe(1); });");
        assert!(d.is_empty());
    }
}
