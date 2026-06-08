//! no-single-promise-in-promise-methods backend.
//!
//! Flags `Promise.all([single])`, `Promise.any([single])`, `Promise.race([single])`.
//! Wrapping a single element in a Promise combinator is unnecessary overhead
//! and reduces readability.

use crate::diagnostic::{Diagnostic, Severity};

const PROMISE_METHODS: &[&str] = &["all", "any", "race"];

/// Count non-spread named children in the array. Returns `None` if any
/// element is a spread element, since `Promise.all([...items])` is valid.
fn single_non_spread_element(array_node: tree_sitter::Node) -> bool {
    let count = array_node.named_child_count();
    if count != 1 {
        return false;
    }
    if let Some(child) = array_node.named_child(0) {
        return child.kind() != "spread_element";
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["Promise"] => |node, source, ctx, diagnostics|
    // callee must be `Promise.{all,any,race}`
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.kind() != "identifier" || obj.utf8_text(source).unwrap_or("") != "Promise" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method_name = prop.utf8_text(source).unwrap_or("");
    if !PROMISE_METHODS.contains(&method_name) {
        return;
    }

    // First argument must be an array with exactly one non-spread element
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 1 {
        return;
    }
    let Some(first_arg) = args.named_child(0) else { return };
    if first_arg.kind() != "array" {
        return;
    }
    if !single_non_spread_element(first_arg) {
        return;
    }

    let pos = first_arg.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-single-promise-in-promise-methods".into(),
        message: format!(
            "Wrapping single-element array with `Promise.{method_name}()` is unnecessary \
             — use the value directly."
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

    #[test]
    fn flags_promise_all_single() {
        let d = run_on("await Promise.all([fetchData()]);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-single-promise-in-promise-methods");
    }

    #[test]
    fn flags_promise_race_single() {
        let d = run_on("await Promise.race([fetchData()]);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_promise_any_single() {
        let d = run_on("await Promise.any([fetchData()]);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_multiple_elements() {
        assert!(run_on("await Promise.all([fetchA(), fetchB()]);").is_empty());
    }

    #[test]
    fn allows_spread_element() {
        assert!(run_on("await Promise.all([...promises]);").is_empty());
    }

    #[test]
    fn allows_promise_all_settled() {
        // allSettled is not in the list — semantics differ
        assert!(run_on("await Promise.allSettled([fetchData()]);").is_empty());
    }

    #[test]
    fn allows_empty_array() {
        assert!(run_on("await Promise.all([]);").is_empty());
    }
}
