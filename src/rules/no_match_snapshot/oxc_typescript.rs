use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// Path segments that identify files dedicated to protocol/contract/serialization
/// testing, where snapshots are the correct tool (they pin a wire format).
const CONTRACT_MARKERS: &[&str] = &["contract", "serial", "wire", "protocol"];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toMatchSnapshot", "toMatchInlineSnapshot"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if is_contract_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };
        let method = mem.property.name.as_str();
        if method != "toMatchSnapshot" && method != "toMatchInlineSnapshot" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{method}()` is a maintenance trap — unrelated \
                 refactors break it and reviewers blindly update \
                 snapshots. Assert on specific fields instead."
            ),
            severity: super::META.severity,
            span: None,
        });
    }
}

fn is_contract_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    CONTRACT_MARKERS.iter().any(|m| s.contains(m))
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
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
}
