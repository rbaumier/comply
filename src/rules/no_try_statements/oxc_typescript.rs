use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, CatchClause, Expression, Statement};
use std::sync::Arc;

pub struct Check;

/// Returns true if `expr` is (or contains, via `&&` / `||` / `!`) an
/// `instanceof` check — used as a signal that the catch routes the error
/// through a typed handler at a library boundary.
fn contains_instanceof(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::BinaryExpression(bin) => {
            if matches!(bin.operator, BinaryOperator::Instanceof) {
                return true;
            }
            contains_instanceof(&bin.left) || contains_instanceof(&bin.right)
        }
        Expression::LogicalExpression(log) => {
            contains_instanceof(&log.left) || contains_instanceof(&log.right)
        }
        Expression::UnaryExpression(un) if un.operator != oxc_ast::ast::UnaryOperator::LogicalNot => {
            contains_instanceof(&un.argument)
        }
        _ => false,
    }
}

/// True if the statement does something more than logging — i.e. it's not
/// a bare `console.log(...)` / `console.error(...)` swallow.
fn is_non_trivial(stmt: &Statement) -> bool {
    match stmt {
        Statement::ExpressionStatement(es) => {
            if let Expression::CallExpression(call) = &es.expression
                && let Expression::StaticMemberExpression(member) = &call.callee
                && let Expression::Identifier(obj) = &member.object
                && obj.name.as_str() == "console"
            {
                return false;
            }
            true
        }
        Statement::EmptyStatement(_) => false,
        Statement::ThrowStatement(_) => false,
        Statement::BlockStatement(b) => b.body.iter().any(is_non_trivial),
        _ => true,
    }
}

/// Heuristic: the catch clause routes the error through a typed handler.
///
/// Matches when an `if (... instanceof X)` (or `&&`/`||`/`!` combo) appears
/// in the catch body and its consequent does something non-trivial.
fn catch_routes_through_typed_handler(handler: &CatchClause) -> bool {
    for stmt in &handler.body.body {
        if let Statement::IfStatement(if_stmt) = stmt
            && contains_instanceof(&if_stmt.test)
            && is_non_trivial(&if_stmt.consequent)
        {
            return true;
        }
    }
    false
}

/// True when the try block wraps a TanStack Query `*.mutateAsync(...)` call.
/// `mutateAsync` throws on error and has no `Result`-based variant at the React
/// call site, so a try/catch around it is legitimate library-boundary control
/// flow (bail out before running post-success effects), not a Result smell.
fn try_wraps_throwing_library_call(block_text: &str) -> bool {
    const THROWING_BUILTINS: &[&str] = &[
        ".mutateAsync(",
        "fetch(",
        "JSON.parse(",
        "localStorage.",
        "sessionStorage.",
    ];
    THROWING_BUILTINS.iter().any(|m| block_text.contains(m))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["try"])
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
        // `try { … } finally { … }` with no catch performs no error handling —
        // the error propagates untouched. The `finally` exists only to
        // guarantee cleanup (clearTimeout, closeDatabase) on every exit path,
        // for which there is no Result-type equivalent. Not a Result smell.
        let Some(handler) = &stmt.handler else {
            return;
        };
        if catch_routes_through_typed_handler(handler) {
            return;
        }
        let block_text = &ctx.source
            [stmt.block.span.start as usize..(stmt.block.span.end as usize).min(ctx.source.len())];
        if try_wraps_throwing_library_call(block_text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-try-statements".into(),
            message: "`try` block \u{2014} prefer Result types or explicit error handling."
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_try_block() {
        let d = run("try { foo(); } catch (e) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-try-statements");
    }

    /// Regression for #576 — `try/finally` with no catch is cleanup-only and
    /// performs no error handling, so it must not flag.
    #[test]
    fn allows_try_finally_without_catch() {
        let d = run("try { return await Promise.race([p, t]); } finally { clearTimeout(id); }");
        assert!(d.is_empty());
    }

    /// A try with a catch (and a trailing finally) is still error handling.
    #[test]
    fn flags_try_catch_finally() {
        let d = run("try { foo(); } catch (e) {} finally { cleanup(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_normal_code() {
        let d = run("const retry = 3;");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_function_call() {
        let d = run("doSomething();");
        assert!(d.is_empty());
    }

    /// Issue #98 — library boundary: third-party `signIn.email(...)` throws,
    /// catch routes the error through `instanceof Error` + typed handler.
    #[test]
    fn skips_catch_with_instanceof_typed_handler() {
        let src = "\
            try {\n\
              await signIn.email(value);\n\
            } catch (error: unknown) {\n\
              if (error instanceof Error) {\n\
                applyProblemErrorToForm(error, target);\n\
              }\n\
            }\n";
        assert!(run(src).is_empty(), "library-boundary catch should be skipped");
    }

    #[test]
    fn flags_catch_with_instanceof_but_only_console_log() {
        let src = "\
            try { foo(); } catch (error) {\n\
              if (error instanceof Error) {\n\
                console.log(error);\n\
              }\n\
            }\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_catch_with_instanceof_in_logical_combo() {
        let src = "\
            try { foo(); } catch (error) {\n\
              if (error instanceof Error && error.message) {\n\
                handle(error);\n\
              }\n\
            }\n";
        assert!(run(src).is_empty());
    }

    /// Regression: a re-throw inside the instanceof branch is not real handling —
    /// the error escapes, so the try block should still be flagged.
    #[test]
    fn still_flags_when_catch_body_only_rethrows() {
        let src = "\
            try { foo(); } catch (e) {\n\
              if (e instanceof X) throw e;\n\
            }\n";
        assert_eq!(run(src).len(), 1);
    }

    /// Regression: `!(e instanceof KnownError)` means the body runs for
    /// non-KnownError errors, which is not typed handling — must still flag.
    #[test]
    fn still_flags_when_instanceof_is_negated() {
        let src = "\
            try { foo(); } catch (e) {\n\
              if (!(e instanceof KnownError)) handle(e);\n\
            }\n";
        assert_eq!(run(src).len(), 1);
    }

    /// Regression for #271 — TanStack Query `mutateAsync` throws and has no
    /// Result variant at the React call site; a try/catch bail-out around it is
    /// legitimate library-boundary control flow.
    #[test]
    fn skips_try_wrapping_mutate_async() {
        let src = "\
            try {\n\
              await editMutation.mutateAsync({ id });\n\
            } catch {\n\
              return;\n\
            }\n";
        assert!(run(src).is_empty(), "mutateAsync boundary try should be skipped");
    }

    #[test]
    fn still_flags_plain_try_without_mutate_async() {
        let src = "try { await save(); } catch { return; }";
        assert_eq!(run(src).len(), 1);
    }

    /// Regression for #576 — wrapping a throwing built-in (`fetch`) to convert
    /// it to a project error type is the correct boundary, not a Result smell.
    #[test]
    fn skips_try_wrapping_fetch() {
        let src = "try { const r = await fetch(url); return r; } catch (e) { throw new ProblemError(e); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_try_wrapping_json_parse() {
        let src = "try { return JSON.parse(raw); } catch (e) { throw new ParseError(e); }";
        assert!(run(src).is_empty());
    }
}
