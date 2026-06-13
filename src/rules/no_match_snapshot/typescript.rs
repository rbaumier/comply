//! no-match-snapshot backend — flag `toMatchSnapshot()` / `toMatchInlineSnapshot()`.
//!
//! Why: snapshot tests are a maintenance trap. They capture the output
//! shape at one moment, then every unrelated refactor breaks them and
//! developers blindly update the snapshot. The test no longer asserts
//! anything specific — it asserts "whatever the code currently produces".
//! Assert on specific fields instead.
//!
//! Exempt: files whose path contains `contract`, `serial`, `wire`, `protocol`,
//! or `snapshot`. The first four pin a protocol/wire-format contract; `snapshot`
//! marks files testing the snapshot mechanism itself (a test framework asserting
//! on its own `toMatchSnapshot` output), where the inline location is intentional.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Path markers that identify files where snapshots are the correct tool:
/// protocol/contract/serialization tests pin a wire format, and files testing
/// the snapshot mechanism itself (a test framework asserting on its own
/// `toMatchSnapshot` output) intentionally embed the exact output inline.
const CONTRACT_MARKERS: &[&str] = &["contract", "serial", "wire", "protocol", "snapshot"];

fn is_contract_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    CONTRACT_MARKERS.iter().any(|m| s.contains(m))
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toMatchSnapshot", "toMatchInlineSnapshot"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if is_contract_file(ctx.path) {
            return;
        }
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "member_expression" {
            return;
        }
        let Some(property) = function.child_by_field_name("property") else {
            return;
        };
        let Ok(method) = property.utf8_text(source_bytes) else {
            return;
        };
        if method != "toMatchSnapshot" && method != "toMatchInlineSnapshot" {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-match-snapshot".into(),
            message: format!(
                "`{method}()` is a maintenance trap — unrelated \
                 refactors break it and reviewers blindly update \
                 snapshots. Assert on specific fields instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_to_match_snapshot() {
        assert_eq!(run_on("expect(x).toMatchSnapshot();").len(), 1);
    }

    #[test]
    fn flags_to_match_inline_snapshot() {
        assert_eq!(run_on("expect(x).toMatchInlineSnapshot('y');").len(), 1);
    }

    #[test]
    fn allows_specific_assertions() {
        assert!(run_on("expect(x.foo).toBe('bar');").is_empty());
    }

    // Regression #992 — snapshots in protocol/contract/serialization test files
    // are the correct tool for pinning wire formats; they must not be flagged.
    #[test]
    fn no_fp_in_contract_test_file() {
        let src = "expect(await action()).toMatchInlineSnapshot(`Object { \"result\": Object { \"data\": Object { \"foo\": \"bar\" } } }`);";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "client.contract.test.ts")
                .is_empty()
        );
    }

    #[test]
    fn no_fp_in_serial_test_file() {
        let src = "expect(x).toMatchSnapshot();";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "src/serial/response.test.ts")
                .is_empty()
        );
    }

    #[test]
    fn no_fp_in_wire_test_file() {
        let src = "expect(x).toMatchInlineSnapshot('y');";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "wire-format.test.ts").is_empty()
        );
    }

    #[test]
    fn no_fp_in_protocol_test_file() {
        let src = "expect(x).toMatchSnapshot();";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "trpc-protocol.test.tsx").is_empty()
        );
    }

    #[test]
    fn still_flags_in_regular_test_file() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "expect(x).toMatchInlineSnapshot('y');",
                "src/app-dir/client.test.tsx"
            )
            .len(),
            1
        );
    }

    // Regression #1392 — a test framework testing its own snapshot output uses
    // inline snapshots intentionally; the file path identifies snapshot as the
    // subject under test, so it must not be flagged.
    #[test]
    fn no_fp_in_snapshot_subject_test_file() {
        let src = "it('renders snapshot', () => { expect(render()).toMatchInlineSnapshot(`\"...\"`); });";
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                src,
                "test/ui/fixtures/snapshot.test.ts"
            )
            .is_empty()
        );
    }
}
