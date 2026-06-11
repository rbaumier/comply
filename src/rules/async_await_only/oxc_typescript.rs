use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True when the receiver chain bottoms out at the literal identifier `z` (Zod).
/// Syntactic only — does not resolve aliased imports (`z as zod`), variable-bound schemas (`const Schema = z.string()`), or nested `.pipe(z.x)`.
fn receiver_is_zod_chain(expr: &Expression) -> bool {
    let mut cur = expr;
    loop {
        match cur {
            Expression::Identifier(id) => return id.name.as_str() == "z",
            Expression::StaticMemberExpression(m) => cur = &m.object,
            Expression::ComputedMemberExpression(m) => cur = &m.object,
            Expression::CallExpression(c) => cur = &c.callee,
            Expression::TSNonNullExpression(n) => cur = &n.expression,
            Expression::ParenthesizedExpression(p) => cur = &p.expression,
            _ => return false,
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".then(", ".catch("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let method = member.property.name.as_str();
        if method != "then" && method != "catch" {
            return;
        }

        // Zod `.catch`/`.then` are schema combinators — flagging them is a false positive.
        if receiver_is_zod_chain(&member.object) {
            return;
        }

        // A terminal `.catch(handler)` directly under `await` or `void` is the
        // canonical error-fallback / fire-and-forget idiom, not a promise chain
        // that should become `try/catch`:
        //   - `await promise.catch(() => fallback)` — default-on-failure
        //   - `void promise.catch(() => {})` — fire-and-forget with handled rejection
        // The alternative (`Promise.allSettled([p])`) is strictly worse.
        if method == "catch" {
            let parent = semantic.nodes().parent_node(node.id()).kind();
            let exempt = matches!(parent, AstKind::AwaitExpression(_))
                || matches!(parent, AstKind::UnaryExpression(u)
                    if u.operator == oxc_ast::ast::UnaryOperator::Void);
            if exempt {
                return;
            }

            // A fire-and-forget `.catch()` statement at module top level (e.g.
            // Angular's canonical `bootstrapApplication(App, appConfig)
            // .catch((err) => console.error(err))` in main.ts) has no enclosing
            // async function to host an `await`, and top-level await is not
            // available in every bundling context — the bare `.catch()`
            // statement is the idiomatic form there. Only the discarded-result
            // statement shape is exempt: a `.catch()` whose value is used
            // (assigned, passed as an argument) is still flagged.
            if matches!(parent, AstKind::ExpressionStatement(_))
                && !semantic.nodes().ancestor_kinds(node.id()).any(|kind| {
                    matches!(
                        kind,
                        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                    )
                })
            {
                return;
            }
        }

        // React.lazy() requires a sync callback returning a Promise — the .then()
        // reshapes the module object and cannot be replaced with await.
        if crate::oxc_helpers::is_react_lazy_factory_then(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{method}()` chain — prefer `async`/`await` for readability."
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
    fn flags_then_chain() {
        assert_eq!(run("foo().then((x) => x + 1);").len(), 1);
    }

    #[test]
    fn flags_unawaited_catch() {
        assert_eq!(run("const p = foo().catch(() => null);").len(), 1);
    }

    #[test]
    fn allows_awaited_catch_fallback() {
        // Regression for issue #561: `await x.catch(() => null)` is the canonical
        // error-fallback on a single awaited operation, already async/await style.
        let src = "async function f() { const b = await response.clone().json().catch(() => null); return b; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_voided_catch_fire_and_forget() {
        // Regression for issue #562: `void p.catch(() => {})` is the canonical
        // fire-and-forget idiom with a handled rejection.
        let src = "void navigator.clipboard.writeText(s).catch(() => {});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_top_level_fire_and_forget_catch() {
        // Regression for issue #978: Angular's canonical standalone bootstrap in
        // main.ts runs at module top level where no async host exists for `await`.
        let src = "bootstrapApplication(App, appConfig).catch((err) => console.error(err));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_fire_and_forget_catch_inside_function() {
        // Inside a function body an `await` host is one `async` keyword away —
        // the top-level exemption does not apply.
        let src = "function f() { foo().catch(() => {}); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_awaited_then_chain() {
        // `.then` directly awaited is still a transform-chain the rule targets.
        let src = "async function f() { return await foo().then((x) => x + 1); }";
        assert_eq!(run(src).len(), 1);
    }
}
