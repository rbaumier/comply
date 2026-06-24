//! OxcCheck backend — flag `throw` in modules importing better-result.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["better-result"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else { return };
        if !ctx.source_contains("better-result") && !ctx.source_contains("@better-result") {
            return;
        }
        if inside_result_try_callback(node, semantic) {
            return;
        }
        // Typed-throw bridge: `throw X.error` re-throws a Result's
        // already-typed ApiError so the framework's error middleware
        // can map it to a Problem response. This is the canonical
        // `unwrapOrThrow(promise)` shape mandated by Amadeo's CLAUDE.md
        // and used by every Elysia handler. The throw IS the helper's
        // contract, not an escape hatch.
        if let Expression::StaticMemberExpression(member) = &throw.argument
            && member.property.name.as_str() == "error"
        {
            return;
        }
        // `: never` throw-helper bridge: a function explicitly annotated to
        // return `never` (e.g. `function throwValidationError(...): never { ... }`)
        // exists solely to throw — TypeScript guarantees it never returns
        // normally, so `return Result.err(...)` is structurally impossible
        // there. These are the typed-error throw-helpers Amadeo's handlers use
        // to bridge a domain failure to the error-handler middleware; the
        // throw is the helper's contract, not an escape hatch (#40).
        if enclosing_function_returns_never(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, throw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "In modules importing better-result, throw is forbidden \u{2014} return Result.err(...) instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk ancestors to check if this node is inside a context where throwing is
/// the expected pattern:
/// - `Result.try(...)` / `Result.tryPromise(...)` — static constructors
/// - `result.match({ ok: ..., err: ... })` — instance combinator whose `err`
///   callback may need to throw to satisfy a third-party API contract
///   (e.g. Better Auth hooks that require throwing APIError).
fn inside_result_try_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind()
            && let Expression::StaticMemberExpression(member) = &call.callee {
                let prop = member.property.name.as_str();
                if (prop == "try" || prop == "tryPromise")
                    && let Expression::Identifier(obj) = &member.object
                        && obj.name.as_str() == "Result" {
                            return true;
                        }
                if prop == "match" {
                    return true;
                }
            }
    }
    false
}

/// Walk ancestors to the nearest enclosing function and report whether its
/// declared return type is the `never` keyword. A `: never`-annotated function
/// can only diverge (throw or loop forever); it cannot `return Result.err(...)`
/// without violating its own signature, so the no-throw remediation does not
/// apply. Recognising this shape exempts the typed-error throw-helpers used to
/// bridge a domain failure into a thrown `ApiError` (#40).
fn enclosing_function_returns_never<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        let return_type = match ancestor.kind() {
            AstKind::Function(func) => func.return_type.as_ref(),
            AstKind::ArrowFunctionExpression(arrow) => arrow.return_type.as_ref(),
            _ => continue,
        };
        return matches!(
            return_type.map(|ann| &ann.type_annotation),
            Some(TSType::TSNeverKeyword(_))
        );
    }
    false
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
    fn flags_bare_throw_in_better_result_module() {
        let src = r#"
            import { Result } from "better-result";
            function f() { throw new Error("oops"); }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_throw_inside_result_try() {
        let src = r#"
            import { Result } from "better-result";
            const r = Result.try(() => { throw new Error("oops"); });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_throw_x_error_bridge() {
        // Regression for rbaumier/comply#40 — `throw result.error` is
        // the canonical Result→typed-throw bridge used by unwrapOrThrow.
        let src = r#"
            import { Result } from "better-result";
            async function unwrapOrThrow<T, E>(p: Promise<Result<T, E>>): Promise<T> {
                const result = await p;
                if (result.isErr()) {
                    throw result.error;
                }
                return result.value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_throw_in_never_returning_throw_helper() {
        // Regression for rbaumier/comply#40 (reopened) — a `: never`
        // throw-helper that throws a typed `ApiError` subclass is the
        // documented Result→throw bridge. The `: never` annotation makes
        // `return Result.err(...)` impossible, so the throw is its contract.
        // Real firing site: validate-new-password.ts:140.
        let src = r#"
            import { Result, TaggedError } from "better-result";
            function throwPasswordValidationError(message: string): never {
              throw new ValidationError({ errors: [], cause: null });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_throw_in_never_returning_arrow_helper() {
        // The bridge shape also applies to an arrow annotated `: never`.
        let src = r#"
            import { Result } from "better-result";
            const fail = (message: string): never => {
              throw new ValidationError({ message });
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_throw_in_value_returning_function() {
        // True positive preserved — a function that returns a value (no
        // `: never` annotation) must still return Result.err(...) instead of
        // throwing. This is the ordinary careless throw the rule targets.
        let src = r#"
            import { Result } from "better-result";
            function loadConfig(): Config {
              throw new ValidationError({ message: "bad config" });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_throw_in_never_returning_method() {
        // A class method annotated `: never` is the same throw-helper bridge.
        // oxc represents a method body as a `Function` node carrying the
        // method's return type, so it is covered by the same ancestor walk.
        let src = r#"
            import { Result } from "better-result";
            class ErrorHelper {
              fail(message: string): never {
                throw new ValidationError({ message });
              }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_throw_in_nested_value_returning_closure() {
        // The exemption is scoped to the *nearest* enclosing function. A bare
        // throw inside an inner closure that itself returns a value must stay
        // flagged even when wrapped in an outer `: never` helper.
        let src = r#"
            import { Result } from "better-result";
            function outer(): never {
              const inner = (): Config => {
                throw new ValidationError({ message: "bad" });
              };
              return inner();
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_throw_inside_match_err_callback() {
        // Regression for #540 — Better Auth hooks require throwing APIError
        // inside the `.match()` err callback; Result-based return is impossible.
        let src = r#"
            import { Result } from "better-result";
            scopeResult.match({
              ok: (scope) => ({ data: { ...session, ...scope } }),
              err: (apiError) => {
                throw new APIError(
                  apiError.status === 403 ? 'FORBIDDEN' : 'INTERNAL_SERVER_ERROR',
                  { ...apiError }
                );
              },
            });
        "#;
        assert!(run(src).is_empty());
    }
}
