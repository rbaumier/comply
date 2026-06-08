//! no-typeof-undefined backend — flag `typeof x === 'undefined'`.

use crate::diagnostic::{Diagnostic, Severity};

/// Browser/DOM globals that are legitimately absent at SSR runtime. The
/// canonical `typeof X === 'undefined'` SSR-detection idiom must not be
/// rewritten to `=== undefined` because TypeScript's lib.dom declares these
/// globals as never-undefined (which makes `no-unnecessary-condition` fire).
const DOM_GLOBALS: &[&str] = &[
    "window",
    "document",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "IntersectionObserver",
    "ResizeObserver",
    "MutationObserver",
    "requestAnimationFrame",
    "MediaQueryList",
    "matchMedia",
    "crypto",
    "performance",
    "indexedDB",
    "WebSocket",
    "Worker",
    "SharedWorker",
];

const GLOBAL_PROXIES: &[&str] = &["globalThis", "window", "self", "global"];

fn is_dom_global_ssr_check(arg: tree_sitter::Node, source: &[u8]) -> bool {
    match arg.kind() {
        "identifier" => {
            let name = arg.utf8_text(source).unwrap_or("");
            DOM_GLOBALS.contains(&name)
        }
        "member_expression" => {
            let Some(prop) = arg.child_by_field_name("property") else { return false };
            let prop_name = prop.utf8_text(source).unwrap_or("");
            if !DOM_GLOBALS.contains(&prop_name) {
                return false;
            }
            let Some(obj) = arg.child_by_field_name("object") else { return false };
            if obj.kind() != "identifier" {
                return false;
            }
            let obj_name = obj.utf8_text(source).unwrap_or("");
            GLOBAL_PROXIES.contains(&obj_name)
        }
        _ => false,
    }
}

crate::ast_check! { on ["binary_expression"] prefilter = ["typeof"] => |node, source, ctx, diagnostics|
    // One side must be a `typeof` unary expression, the other must be "undefined" string.
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    fn typeof_operand<'a>(n: tree_sitter::Node<'a>, source: &[u8]) -> Option<tree_sitter::Node<'a>> {
        if n.kind() != "unary_expression" {
            return None;
        }
        let op = n.child_by_field_name("operator")?;
        if op.utf8_text(source).unwrap_or("") != "typeof" {
            return None;
        }
        n.child_by_field_name("argument")
    }

    let typeof_arg = typeof_operand(left, source).or_else(|| typeof_operand(right, source));
    let Some(arg) = typeof_arg else { return };

    let is_undefined_string = |n: tree_sitter::Node| -> bool {
        if n.kind() != "string" {
            return false;
        }
        let text = n.utf8_text(source).unwrap_or("");
        text == "'undefined'" || text == "\"undefined\""
    };

    if !is_undefined_string(left) && !is_undefined_string(right) {
        return;
    }

    // SSR-detection idiom: `typeof globalThis.window === 'undefined'`,
    // `typeof window === 'undefined'`, etc. Keep `typeof` so the rewrite
    // does not collide with `no-unnecessary-condition`.
    if is_dom_global_ssr_check(arg, source) {
        return;
    }

    // Only flag when the operand is guaranteed to be a declared binding.
    // `typeof x === 'undefined'` where `x` is a bare identifier is the only
    // safe way to test a possibly-undeclared variable — `x === undefined`
    // throws ReferenceError.
    let safe_to_rewrite = matches!(
        arg.kind(),
        "member_expression" | "subscript_expression"
    );
    if !safe_to_rewrite {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-typeof-undefined".into(),
        message: "Prefer `=== undefined` over `typeof … === 'undefined'` when \
                  the operand is a property access (which cannot throw \
                  ReferenceError).".into(),
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

    #[test]
    fn flags_typeof_member_expression() {
        let d =
            crate::rules::test_helpers::run_rule(&Check, "if (typeof obj.foo === 'undefined') {}", "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-typeof-undefined");
    }

    #[test]
    fn flags_typeof_member_expression_double_quotes() {
        let d =
            crate::rules::test_helpers::run_rule(&Check, r#"if (typeof obj.foo === "undefined") {}"#, "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_typeof_subscript_expression() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (typeof arr[0] === 'undefined') {}", "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_typeof_bare_identifier() {
        // `x` may not be declared — `x === undefined` would throw.
        // `typeof x === 'undefined'` is the only safe check.
        let d = crate::rules::test_helpers::run_rule(&Check, "if (typeof x === 'undefined') {}", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_direct_undefined_comparison() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (x === undefined) {}", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_typeof_for_other_types() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (typeof x === 'string') {}", "t.ts");
        assert!(d.is_empty());
    }

    // Regression for #209 — SSR guards on DOM globals must not fire.
    #[test]
    fn allows_typeof_globalthis_window() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (typeof globalThis.window === 'undefined') {}", "t.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_bare_window() {
        let d =
            crate::rules::test_helpers::run_rule(&Check, "if (typeof window === 'undefined') {}", "t.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_bare_document() {
        let d =
            crate::rules::test_helpers::run_rule(&Check, "if (typeof document === 'undefined') {}", "t.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_navigator_negated() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (typeof navigator !== 'undefined') {}", "t.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_globalthis_document() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (typeof globalThis.document === 'undefined') {}", "t.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn flags_typeof_non_dom_property_access() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (typeof someObj.someProp === 'undefined') {}", "t.ts");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_typeof_globalthis_non_dom() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (typeof globalThis.myCustomGlobal === 'undefined') {}", "t.ts");
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
