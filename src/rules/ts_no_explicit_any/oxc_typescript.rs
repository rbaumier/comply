//! ts-no-explicit-any oxc backend — flag TSAnyKeyword. tsd-style type-level
//! test files ([`crate::rules::file_ctx::FileCtx::is_type_test_file`]) are
//! exempt: there `any` is a required test vector — it verifies how a type
//! distributes over `any`, which differs from `unknown`/`never` — not a
//! production escape hatch.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAnyKeyword]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.is_type_test_file() {
            return;
        }
        let AstKind::TSAnyKeyword(kw) = node.kind() else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, kw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Explicit `any` disables type checking — prefer `unknown` (forces \
                      narrowing at the use site) or a precise type."
                .into(),
            severity: Severity::Error,
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Builds the real `FileCtx` from `path`, which the type-test-file guard reads.
    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path)
    }

    #[test]
    fn flags_param_typed_any() {
        let src = "function f(x: any): number { return 0; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_as_any_cast() {
        let src = "const x = something as any;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_unknown() {
        let src = "function f(x: unknown): number { return 0; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_any_in_regular_src() {
        let src = "function f(x: any): number { return 0; }";
        assert_eq!(run_at(src, "src/foo.ts").len(), 1);
    }

    #[test]
    fn ignores_any_in_every_type_test_file_convention() {
        // One shared spelling of "this is a tsd/dtslint type test", so a
        // convention recognised for one type-test rule is recognised for all.
        let src = "function f(x: any): number { return 0; }";
        for path in [
            "test-d/and.ts",
            "and.test-d.ts",
            "and.test-d.tsx",
            "and.types-test.ts",
            "test-tsd/and.ts",
            "dtslint/and.ts",
            "src/addDays/test.tp.ts",
        ] {
            assert!(
                run_at(src, path).is_empty(),
                "expected `any` to be exempt in type-test file {path}"
            );
        }
    }
}
