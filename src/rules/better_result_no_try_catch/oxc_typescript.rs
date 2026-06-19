//! Flags a `try` statement that has a `catch` clause in a better-result module,
//! since `Result.try({ try, catch })` is the idiomatic replacement. A
//! `try`/`finally` with no `catch` is a resource-cleanup idiom (the `finally`
//! runs on every path and there is no `catch` to convert), so it is left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn imports_better_result(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "better-result") || crate::oxc_helpers::source_contains(source, "@better-result")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["better-result"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(stmt) = node.kind() else {
            return;
        };
        // `Result.try({ try, catch })` replaces a try/CATCH. A try/finally with no
        // catch is a resource-cleanup idiom — the `finally` must run on every path and
        // there is no `catch` to convert — so it is not a better-result rewrite
        // candidate.
        if stmt.handler.is_none() {
            return;
        }
        if !imports_better_result(ctx.source) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "better-result-no-try-catch".into(),
            message: "Replace try/catch with Result.try({ try, catch }) in better-result modules."
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_try_catch_in_better_result_module() {
        let src = "import { Result } from 'better-result';\ntry { f(); } catch (e) { g(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_try_catch_when_no_better_result() {
        let src = "try { f(); } catch (e) { g(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_try_finally_with_no_catch() {
        // Regression #4228: a resource-cleanup try/finally with no catch has no
        // `Result.try` rewrite — the `finally` runs on every path.
        let src = "import { Result } from 'better-result';\nasync function withTimeout(p, ms) {\n  const timer = { id: undefined };\n  try {\n    return Result.ok(await p);\n  } finally {\n    clearTimeout(timer.id);\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_try_catch_finally_in_better_result_module() {
        let src = "import { Result } from 'better-result';\ntry { f(); } catch (e) { g(); } finally { h(); }";
        assert_eq!(run(src).len(), 1);
    }
}
