//! elysia-resolve-outside-guard backend — flag top-level `.resolve(`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["\".resolve\""] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if ctx.source_contains(".guard(") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".resolve") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-resolve-outside-guard".into(),
        message: "`.resolve()` is used outside `.guard()` — derived values leak to every route in the chain.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_top_level_resolve() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().resolve(({ headers }) => ({ user: headers.x }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_resolve_inside_guard() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().guard({}, app => app.resolve(({ headers }) => ({ user: headers.x })));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "promise.resolve(1);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
