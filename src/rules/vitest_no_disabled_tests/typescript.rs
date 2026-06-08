//! vitest-no-disabled-tests — flag `xtest` / `xit` / `xdescribe` identifiers
//! as well as chained `test.skip` / `describe.skip` / `it.skip`.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const VITEST_IMPORTS: &[&str] = &["from 'vitest'", "from \"vitest\""];

fn has_vitest_import(source: &[u8]) -> bool {
    let src = std::str::from_utf8(source).unwrap_or("");
    VITEST_IMPORTS.iter().any(|p| src.contains(p))
}

const DISABLED_IDENTIFIERS: &[&str] = &["xtest", "xit", "xdescribe"];
const TEST_FNS: &[&str] = &["test", "it", "describe"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) && !has_vitest_import(source) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };

    match callee.kind() {
        "identifier" => {
            let name = callee.utf8_text(source).unwrap_or("");
            if DISABLED_IDENTIFIERS.contains(&name) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
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
                    path: std::sync::Arc::clone(&ctx.path_arc),
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "app.test.ts")
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
        let d = crate::rules::test_helpers::run_rule(&Check, "xtest('a', () => {});", "src/util.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_skip_with_vitest_import_no_marker() {
        let d = crate::rules::test_helpers::run_rule(&Check, "import { it } from 'vitest';\nit.skip('login', () => {});", "tests/login.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "vitest-no-disabled-tests");
    }

    #[test]
    fn ignores_no_marker_no_import() {
        let d = crate::rules::test_helpers::run_rule(&Check, "it.skip('login', () => {});", "tests/login.ts");
        assert!(d.is_empty());
    }
}
