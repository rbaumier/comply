//! react-no-find-in-map-loop OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::oxc_helpers::{
    byte_offset_to_line_col, callback_first_param_name, receiver_root_identifier,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// The rule's rationale is render-path (O(n²)) cost, so it only applies to
/// React code: `.tsx`/`.jsx` files (JSX implies React) or a `.ts`/`.js` module
/// that imports React. Plain backend/server TypeScript is out of scope.
fn in_react_context(ctx: &CheckCtx) -> bool {
    matches!(ctx.lang, Language::Tsx) || crate::oxc_helpers::imports_react(ctx.source)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["find", "filter"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !in_react_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be a `.find(...)` or `.filter(...)` member call.
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };
        let method = mem.property.name.as_str();
        if method != "find" && method != "filter" {
            return;
        }

        // Check if it's inside a loop or .map() callback.
        let receiver_root = receiver_root_identifier(&mem.object);
        if !flagged_inside_loop_or_map(node, semantic, receiver_root.as_deref()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.find`/`.filter` inside a `.map` or loop — O(n\u{b2}). \
                      Build a `Map` once and look up inside the loop."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk up from `node` to determine if it's inside a loop or `.map()` callback.
///
/// The find/filter runs per-iteration only when its nearest enclosing function
/// is directly the callback of the loop/`.map()`. An intervening nested function
/// (event handler such as `onRemove`, a `useCallback`, any deferred closure
/// stored in a prop/variable) means the call is deferred — it runs once on user
/// interaction, not once per `.map()` iteration — so there is no O(n²) render
/// cost. We bail the moment the walk crosses a function boundary that is not the
/// loop/map's own callback.
fn flagged_inside_loop_or_map(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    receiver_root: Option<&str>,
) -> bool {
    let mut current = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(current);
        if parent_id == current {
            return false;
        }
        let child = current;
        current = parent_id;
        let parent = semantic.nodes().get_node(current);
        match parent.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Crossing a function boundary that is itself a loop/`.map()`
                // callback is the expected per-iteration case — keep walking up.
                // Any other enclosing function (handler, deferred closure) means
                // the find/filter is deferred, not per-iteration: stop.
                if !is_loop_or_map_callback(current, semantic) {
                    return false;
                }
            }
            AstKind::CallExpression(call) => {
                if is_map_call(call) {
                    // Distinguish how the walk reached this `.map`. If we ascended
                    // from its callee (the `X.map` member expression), the
                    // find/filter is in the receiver chain — `arr.filter(...).map(...)`
                    // — i.e. downstream chaining (two sequential O(n) passes), not
                    // nesting. If we ascended from within the arguments (the
                    // callback subtree), it's genuine per-iteration nesting.
                    let child_span = semantic.nodes().get_node(child).kind().span();
                    if child_span == call.callee.span() {
                        return false;
                    }
                    // Inside the callback. If the find/filter receiver root matches
                    // the map callback param, it's not the O(n^2) pattern.
                    let param = callback_first_param_name(call);
                    match (receiver_root, param.as_deref()) {
                        (Some(recv), Some(p)) if recv == p => {
                            // derived from current iteration item — keep looking up
                        }
                        _ => return true,
                    }
                }
            }
            _ => {}
        }
    }
}

/// True when the function node `func_id` is the callback argument of a `.map()`
/// call — i.e. its parent is a `.map()` CallExpression and the function sits in
/// that call's arguments. This is the one function boundary that runs per
/// iteration; every other enclosing function defers execution.
fn is_loop_or_map_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let func_span = semantic.nodes().get_node(func_id).kind().span();
    let parent = semantic.nodes().parent_node(func_id);
    let AstKind::CallExpression(call) = parent.kind() else {
        return false;
    };
    if !is_map_call(call) {
        return false;
    }
    call.arguments
        .iter()
        .filter_map(oxc_ast::ast::Argument::as_expression)
        .any(|arg| arg.span() == func_span)
}

fn is_map_call(call: &oxc_ast::ast::CallExpression) -> bool {
    if let Expression::StaticMemberExpression(mem) = &call.callee {
        mem.property.name == "map"
    } else {
        false
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
    

    const NESTED: &str = r#"items.map(i => others.find(o => o.id === i.id));"#;

    #[test]
    fn flags_in_tsx() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, NESTED, "t.tsx").len(), 1);
    }

    // Regression for #280: a plain `.ts` module with no React import is not
    // render-path code — the rule must stay silent there.
    #[test]
    fn allows_plain_ts_without_react() {
        assert!(crate::rules::test_helpers::run_rule(&Check, NESTED, "service.ts").is_empty());
    }

    #[test]
    fn flags_ts_that_imports_react() {
        let src = format!("import {{ useMemo }} from \"react\";\n{NESTED}");
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, &src, "hook.ts").len(), 1);
    }

    // Regression for #911: a spread argument to .map() made `Argument::to_expression()` panic.
    #[test]
    fn does_not_panic_on_spread_arg_in_map() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "arr.map(...fns)", "t.tsx").is_empty());
    }

    // True positive: a `.filter()` directly in the map callback body (no
    // intervening function) is per-iteration — must still flag.
    #[test]
    fn flags_filter_directly_in_map_callback() {
        let src = "items.map(i => others.filter(o => o.id === i.id));";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // Regression for #3936: a `.filter()` inside a nested deferred event handler
    // (`onClick`) defined within the map callback runs once per user click, not
    // once per `.map()` iteration — no O(n²) render cost, so it must not flag.
    #[test]
    fn allows_filter_in_nested_event_handler() {
        let src = "arr.map((item) => <X onClick={() => other.filter((i) => i !== item)} />);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // True positive: a `.filter()` directly in a `for`-loop body (no
    // intervening function) is per-iteration — must still flag.
    #[test]
    fn flags_filter_directly_in_for_loop() {
        let src = "for (const x of xs) { others.filter(o => o.id === x.id); }";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // Regression for #3936: a `.filter()` inside a deferred closure defined in a
    // `for`-loop body runs on later invocation, not per-iteration — must not flag.
    #[test]
    fn allows_filter_in_deferred_closure_in_for_loop() {
        let src = "for (const x of xs) { register(() => others.filter(o => o.id === x.id)); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #3745: chained `arr.filter(...).map(...)` is two sequential
    // O(n) passes — the `.filter` is the `.map` receiver, not nested in its
    // callback. The walk reaches `.map` via its callee, so it must not flag.
    #[test]
    fn allows_chained_filter_then_map() {
        let src = "const r = data.filter((env) => ids.includes(env.id)).map((env) => env.name);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Regression for #3745: a longer chain `arr.filter(...).map(...).join(...)`
    // is still pure downstream chaining — must not flag.
    #[test]
    fn allows_longer_filter_map_chain() {
        let src = r#"const r = arr.filter((x) => x.ok).map((x) => x.id).join(",");"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    // Anti-over-broad guard for #3745: a genuinely nested `.find` inside the map
    // callback, with a downstream `.name` access on the find result, is still
    // O(n²). The fix only distinguishes callee vs arguments AT the `.map` call,
    // not at every member step, so this must still flag.
    #[test]
    fn flags_nested_find_with_downstream_access() {
        let src = "const r = items.map((i) => others.find((o) => o.id === i.id).name);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // Regression for #3936: the mantine MultiSelect shape — a `.filter()` inside
    // an `onRemove` arrow nested in the map callback. Deferred, must not flag.
    #[test]
    fn allows_filter_in_nested_onremove_handler() {
        let src = r#"
            const values = _value.map((item, index) => (
              <Pill
                onRemove={() => {
                  setValue(_value.filter((i) => item !== i));
                }}
              />
            ));
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }
}
