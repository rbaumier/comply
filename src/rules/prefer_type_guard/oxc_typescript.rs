//! prefer-type-guard oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// True when some `return` statement inside `[start, end)` returns an
/// expression that contains `typeof` or `instanceof`.
///
/// A genuine type-predicate candidate *returns* the type check itself
/// (`return x instanceof Foo`). When `instanceof`/`typeof` only appears in an
/// `if` condition used for branching — while the returns are unrelated boolean
/// expressions — the function discriminates on more than the type and a
/// `x is T` predicate would be semantically wrong.
fn returns_a_type_check(
    semantic: &oxc_semantic::Semantic,
    source: &str,
    start: u32,
    end: u32,
) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::ReturnStatement(ret) = n.kind() else { return false };
        if ret.span.start < start || ret.span.end > end {
            return false;
        }
        let Some(arg) = &ret.argument else { return false };
        let span = arg.span();
        let text = &source[span.start as usize..span.end as usize];
        text.contains("typeof ") || text.contains("instanceof ")
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Function(func) = node.kind() else {
            return;
        };

        // Must have a name starting with "is" + uppercase.
        let Some(id) = &func.id else { return };
        let name = id.name.as_str();
        if !name.starts_with("is") {
            return;
        }
        let after_is = &name[2..];
        if after_is.is_empty() || !after_is.starts_with(|c: char| c.is_ascii_uppercase()) {
            return;
        }

        // Return type must be `: boolean` (not a type predicate).
        let Some(ret) = &func.return_type else { return };
        let rt_span = ret.span;
        let rt_text = &ctx.source[rt_span.start as usize..rt_span.end as usize];
        let rt_inner = rt_text.trim().strip_prefix(':').unwrap_or(rt_text.trim()).trim();
        if rt_inner != "boolean" {
            return;
        }

        // Only flag when a `return` directly yields a type-check expression.
        let Some(body) = &func.body else { return };
        if !returns_a_type_check(semantic, ctx.source, body.span.start, body.span.end) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, func.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function `isX` returns `boolean` with type checks \u{2014} use a type predicate (`x is Type`) instead.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_returned_typeof() {
        assert_eq!(
            run("function isString(x: unknown): boolean { return typeof x === \"string\"; }").len(),
            1
        );
    }

    #[test]
    fn flags_returned_instanceof() {
        assert_eq!(
            run("function isError(x: unknown): boolean { return x instanceof Error; }").len(),
            1
        );
    }

    #[test]
    fn allows_instanceof_used_for_branching() {
        // Regression for issue #567: `instanceof` gates a branch, but the returns
        // are unrelated booleans (returns `true` for all non-ProblemErrors), so a
        // `error is ProblemError` predicate would be semantically wrong.
        let src = "function isUnexpectedError(error: Error): boolean {\n\
                   if (error instanceof ProblemError) { return error.problem.status >= 500; }\n\
                   return true;\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_type_predicate() {
        assert!(
            run("function isString(x: unknown): x is string { return typeof x === \"string\"; }")
                .is_empty()
        );
    }
}
