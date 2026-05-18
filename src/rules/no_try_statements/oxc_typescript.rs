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
        if let Some(handler) = &stmt.handler
            && catch_routes_through_typed_handler(handler)
        {
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
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_try_block() {
        let d = run("try { foo(); } catch (e) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-try-statements");
    }

    #[test]
    fn flags_try_finally() {
        let d = run("try { foo(); } finally {}");
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
}
