//! ts-no-explicit-any oxc backend — flag TSAnyKeyword. tsd-style type-level
//! test files (`test-d/` directory, `*.test-d.{ts,tsx}`, `*.types-test.{ts,tsx}`)
//! are exempt: there `any` is a required test vector, not a production type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// True when `path` is a tsd-style type-level test file, where `any` is a
/// required test vector (verifying how a type distributes over `any`, which
/// differs from `unknown`/`never`) rather than a production escape hatch.
/// tsd convention: files under a `test-d/` directory, or named
/// `*.test-d.{ts,tsx}` / `*.types-test.{ts,tsx}`.
fn is_tsd_type_test_file(path: &std::path::Path) -> bool {
    if path.components().any(|c| c.as_os_str() == "test-d") {
        return true;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            name.ends_with(".test-d.ts")
                || name.ends_with(".test-d.tsx")
                || name.ends_with(".types-test.ts")
                || name.ends_with(".types-test.tsx")
        })
}

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
        if is_tsd_type_test_file(ctx.path) {
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_any_in_test_d_directory() {
        let src = "function f(x: any): number { return 0; }";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "test-d/and.ts");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_any_in_test_d_suffixed_file() {
        let src = "function f(x: any): number { return 0; }";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "and.test-d.ts");
        assert!(diags.is_empty());
    }
}
