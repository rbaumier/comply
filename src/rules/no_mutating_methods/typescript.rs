//! no-mutating-methods backend — flag calls to array mutating methods.
//!
//! We match any `x.method(...)` call where `method` is a known
//! mutating array method. This is a name-based heuristic — we cannot
//! resolve the receiver's type — but these names are overwhelmingly
//! used on arrays, and each has an explicit non-mutating alternative.

use crate::diagnostic::{Diagnostic, Severity};

const MUTATING: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Ok(name) = prop.utf8_text(source) else { return };
    if !MUTATING.contains(&name) {
        return;
    }
    // .fill() on a chained call (e.g. page.getByLabel(...).fill()) is almost
    // certainly Playwright/Locator.fill, not Array.fill.
    if name == "fill" {
        if let Some(object) = callee.child_by_field_name("object") {
            if matches!(object.kind(), "call_expression" | "member_expression") {
                return;
            }
        }
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-mutating-methods",
        format!(
            "`.{name}()` mutates the array in place — use a non-mutating alternative (spread, `slice`, `toSorted`, `toReversed`, `toSpliced`, `filter`, `map`, `concat`)."
        ),
        Severity::Warning,
    ));
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

    #[test]
    fn flags_push() {
        assert_eq!(run_on("arr.push(1);").len(), 1);
    }

    #[test]
    fn flags_sort() {
        assert_eq!(run_on("arr.sort();").len(), 1);
    }

    #[test]
    fn flags_splice() {
        assert_eq!(run_on("arr.splice(0, 1);").len(), 1);
    }

    #[test]
    fn flags_reverse() {
        assert_eq!(run_on("arr.reverse();").len(), 1);
    }

    #[test]
    fn allows_non_mutating_alternatives() {
        assert!(run_on("const next = [...arr, 1];").is_empty());
        assert!(run_on("arr.toSorted();").is_empty());
        assert!(run_on("arr.toReversed();").is_empty());
        assert!(run_on("arr.slice(0, 1);").is_empty());
        assert!(run_on("arr.map(x => x + 1);").is_empty());
    }

    #[test]
    fn ignores_plain_function_call() {
        assert!(run_on("push(arr, 1);").is_empty());
    }

    #[test]
    fn allows_chained_fill_playwright() {
        assert!(run_on(r#"page.getByLabel("Email").fill(user.email);"#).is_empty());
    }

    #[test]
    fn still_flags_direct_fill() {
        assert_eq!(run_on("arr.fill(0);").len(), 1);
    }

    #[test]
    fn allows_member_expression_fill_playwright() {
        assert!(run_on(r#"this.input.fill(title);"#).is_empty());
    }
}
