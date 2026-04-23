//! vitest-no-disabled-tests — flag `xtest` / `xit` / `xdescribe` identifiers
//! as well as chained `test.skip` / `describe.skip` / `it.skip`.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const DISABLED_IDENTIFIERS: &[&str] = &["xtest", "xit", "xdescribe"];
const TEST_FNS: &[&str] = &["test", "it", "describe"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };

    match callee.kind() {
        "identifier" => {
            let name = callee.utf8_text(source).unwrap_or("");
            if DISABLED_IDENTIFIERS.contains(&name) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "vitest-no-disabled-tests".into(),
                    message: format!("`{name}` disables the test — re-enable or remove it."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        "member_expression" => {
            let Some(obj) = callee.child_by_field_name("object") else { return };
            let Some(prop) = callee.child_by_field_name("property") else { return };
            let obj_text = obj.utf8_text(source).unwrap_or("");
            let prop_text = prop.utf8_text(source).unwrap_or("");

            if TEST_FNS.contains(&obj_text) && prop_text == "skip" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "vitest-no-disabled-tests".into(),
                    message: format!("`{obj_text}.skip(...)` disables the test — re-enable or remove it."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, "app.test.ts")
    }

    #[test]
    fn flags_xtest() {
        let d = run_ts("xtest('broken', () => { expect(1).toBe(1); });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "vitest-no-disabled-tests");
    }

    #[test]
    fn flags_xit() {
        let d = run_ts("xit('broken', () => {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_xdescribe() {
        let d = run_ts("xdescribe('suite', () => {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_test_skip() {
        let d = run_ts("test.skip('broken', () => {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_regular_test() {
        let d = run_ts("test('works', () => { expect(1).toBe(1); });");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = run_ts_with_path("xtest('a', () => {});", &Check, "src/util.ts");
        assert!(d.is_empty());
    }
}
