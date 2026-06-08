//! no-useless-fallback-in-spread backend — flag `{...(foo || {})}` and
//! `{...(foo ?? {})}`. Spreading `undefined`/`null` in an object literal
//! is already a no-op, so the `|| {}` / `?? {}` fallback is unnecessary.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["spread_element"] => |node, source, ctx, diagnostics|
    // We look for spread_element nodes inside object literals whose
    // argument is a logical expression (`||` or `??`) with `{}` as the
    // right-hand side.
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "object" {
        return;
    }

    // The spread's argument — may be a parenthesized_expression wrapping
    // the logical expression, or the logical expression directly.
    let Some(argument) = node.named_child(0) else { return };

    // Unwrap one layer of parentheses if present.
    let inner = if argument.kind() == "parenthesized_expression" {
        match argument.named_child(0) {
            Some(c) => c,
            None => return,
        }
    } else {
        argument
    };

    // Must be a binary_expression with `||` or `??` operator.
    if inner.kind() != "binary_expression" {
        return;
    }

    let Some(operator_node) = inner.child_by_field_name("operator") else { return };
    let op = operator_node.utf8_text(source).unwrap_or("");
    if op != "||" && op != "??" {
        return;
    }

    let Some(right) = inner.child_by_field_name("right") else { return };

    // The right side must be an empty object literal `{}`.
    if right.kind() != "object" {
        return;
    }
    if right.named_child_count() != 0 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-useless-fallback-in-spread".into(),
        message: format!(
            "The `{op} {{}}` fallback is unnecessary — spreading \
             `undefined`/`null` in an object literal is a no-op."
        ),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // ---- flags useless fallback ----

    #[test]
    fn flags_or_empty_object() {
        assert_eq!(run_on("const x = {...(foo || {})};").len(), 1);
    }

    #[test]
    fn flags_nullish_coalescing_empty_object() {
        assert_eq!(run_on("const x = {...(foo ?? {})};").len(), 1);
    }

    // ---- allows correct usage ----

    #[test]
    fn allows_spread_variable() {
        assert!(run_on("const x = {...foo};").is_empty());
    }

    #[test]
    fn allows_non_empty_fallback() {
        assert!(run_on("const x = {...(foo || {a: 1})};").is_empty());
    }

    #[test]
    fn allows_or_in_non_spread_context() {
        assert!(run_on("const x = foo || {};").is_empty());
    }

    #[test]
    fn allows_spread_in_array() {
        // This rule only applies to object spreads.
        assert!(run_on("const x = [...(foo || [])];").is_empty());
    }
}
