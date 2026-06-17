use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new URL"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else {
            return;
        };

        let Some(handler) = &try_stmt.handler else {
            return;
        };

        // The try/catch is only replaceable by the boolean `URL.canParse(url)`
        // when its body is a *pure* validity check: every `new URL(...)` built
        // in the try-body must have its result discarded. `URL.canParse` returns
        // only a boolean, so a body that assigns, returns or member-accesses the
        // parsed URL cannot be expressed by it. A discarded construction is one
        // whose direct parent is an `ExpressionStatement` (`new URL(s);`); any
        // other parent (`return`, assignment, member access, call argument, …)
        // means the value is used.
        if !try_body_only_discards_urls(try_stmt.block.span, semantic) {
            return;
        }

        let catch_text =
            &ctx.source[handler.body.span.start as usize..handler.body.span.end as usize];

        let is_validation_pattern = catch_text.contains("return false")
            || catch_text.contains("return null")
            || catch_text.contains("return undefined");

        if !is_validation_pattern {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, try_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `URL.canParse(url)` instead of try-catch with `new URL()`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the try-body builds at least one `new URL(...)` and *every* such
/// construction has its result discarded — i.e. it sits directly in
/// expression-statement position. Returns `false` if any `new URL(...)` result
/// is used (assigned, returned, member-accessed, passed as an argument, …) or
/// if the body builds no `URL` at all.
fn try_body_only_discards_urls<'a>(
    block_span: oxc_span::Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_ast::ast::Expression;

    let mut saw_url = false;
    for n in semantic.nodes().iter() {
        let AstKind::NewExpression(new_expr) = n.kind() else {
            continue;
        };
        let Expression::Identifier(callee) = &new_expr.callee else {
            continue;
        };
        if callee.name.as_str() != "URL" {
            continue;
        }
        // Restrict to constructions lexically inside the try-block.
        if new_expr.span.start < block_span.start || new_expr.span.end > block_span.end {
            continue;
        }
        saw_url = true;
        // Used (non-discarded) the moment the direct parent is anything other
        // than an `ExpressionStatement`.
        if !matches!(
            semantic.nodes().parent_kind(n.id()),
            AstKind::ExpressionStatement(_)
        ) {
            return false;
        }
    }
    saw_url
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

    // ---- genuine pure-validation patterns: MUST still flag ----

    #[test]
    fn flags_bare_new_url_then_return_true() {
        let code = r#"
            function isValidUrl(s) {
                try { new URL(s); return true; }
                catch { return false; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_bare_new_url_discarded() {
        let code = r#"
            function isValidUrl(s) {
                try { new URL(s); }
                catch { return false; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }

    // ---- #3996 regressions: the constructed URL is USED -> must NOT flag ----

    #[test]
    fn ignores_assigned_url_used_downstream() {
        // axios shouldBypassProxy.js:162 — parsed URL consumed after the try.
        let code = r#"
            function check(location) {
                let parsed;
                try { parsed = new URL(location); }
                catch (_err) { return false; }
                return parsed.hostname;
            }
        "#;
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_member_access_on_constructed_urls() {
        // axios http.js:196 — `new URL(a).origin === new URL(b).origin`.
        let code = r#"
            function sameOrigin(a, b) {
                try { return new URL(a).origin === new URL(b).origin; }
                catch (e) { return false; }
            }
        "#;
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_const_binding() {
        let code = r#"
            function f(s) {
                try { const u = new URL(s); use(u); }
                catch (e) { return false; }
            }
        "#;
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_returned_url() {
        let code = r#"
            function parseUrl(s) {
                try { return new URL(s); }
                catch { return null; }
            }
        "#;
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_url_passed_as_argument() {
        let code = r#"
            function f(s) {
                try { record(new URL(s)); }
                catch { return false; }
            }
        "#;
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    // ---- mixed: one discarded, one used -> must NOT flag ----

    #[test]
    fn ignores_when_any_url_is_used() {
        let code = r#"
            function f(s, t) {
                try { new URL(s); const u = new URL(t); use(u); }
                catch { return false; }
            }
        "#;
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    // ---- shape guards ----

    #[test]
    fn ignores_try_catch_without_validation_catch() {
        let code = r#"
            try { new URL(url); }
            catch (e) { console.error(e); }
        "#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn ignores_url_canparse() {
        assert!(run("const valid = URL.canParse(url);").is_empty());
    }
}
