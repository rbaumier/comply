//! OXC backend for arguments-order.
//!
//! Flags call sites whose identifier arguments match the callee's parameter
//! names in reversed order (a likely accidental swap). A call is exempt when it
//! is a branch of a ternary chain in which a sibling call to the same function
//! passes the same identifier arguments in exactly reversed order
//! (`cond ? f(a, b) : … : f(b, a)`) — the deliberate argument-reversal idiom for
//! selecting sort order or normalizing which operand comes first, not an
//! accidental swap.

use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // 1. Collect function declarations with their parameter names
        let mut signatures: FxHashMap<String, Vec<String>> = FxHashMap::default();
        for node in semantic.nodes().iter() {
            if let AstKind::Function(func) = node.kind()
                && let Some(ref id) = func.id {
                    let params = extract_param_names(func);
                    if !params.is_empty() {
                        signatures.insert(id.name.to_string(), params);
                    }
                }
        }

        // 2. Merge exported function params from ImportIndex
        let index = ctx.project.import_index();
        for imp in index.get_imports(ctx.path) {
            let Some(src_path) = &imp.source_path else {
                continue;
            };
            for export in index.get_exports(src_path) {
                if export.name == imp.imported_name && !export.params.is_empty() {
                    signatures.insert(imp.local_name.clone(), export.params.clone());
                }
            }
        }

        if signatures.is_empty() {
            return diagnostics;
        }

        // 3. Check all call sites
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            let name = callee.name.as_str();
            let Some(params) = signatures.get(name) else {
                continue;
            };

            if in_reversed_arg_ternary(node, semantic, name) {
                continue;
            }

            check_call_args(call, name, params, ctx, &mut diagnostics);
        }

        diagnostics
    }
}

fn extract_param_names(func: &oxc_ast::ast::Function) -> Vec<String> {
    let mut result = Vec::new();
    for param in &func.params.items {
        if let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = param.pattern {
            result.push(id.name.to_string());
        }
    }
    result
}

/// Identifier names of a call's arguments. Non-identifier arguments map to a
/// `None` slot so positions still line up for a reversal comparison.
fn arg_names_of_call(call: &oxc_ast::ast::CallExpression) -> Vec<Option<String>> {
    call.arguments
        .iter()
        .map(|a| match a {
            oxc_ast::ast::Argument::Identifier(id) => Some(id.name.to_string()),
            _ => None,
        })
        .collect()
}

/// Byte span of the outermost `ConditionalExpression` reachable from `node` by
/// following parent `ConditionalExpression` links — the full nested-ternary chain
/// enclosing the call. The caller guarantees `node`'s direct parent is a
/// `ConditionalExpression`.
fn enclosing_conditional_chain_span<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> (u32, u32) {
    let nodes = semantic.nodes();
    let mut cond = nodes.parent_node(node.id());
    loop {
        let grandparent = nodes.parent_node(cond.id());
        if matches!(grandparent.kind(), AstKind::ConditionalExpression(_)) {
            cond = grandparent;
        } else {
            break;
        }
    }
    match cond.kind() {
        AstKind::ConditionalExpression(c) => (c.span.start, c.span.end),
        _ => unreachable!("caller guarantees node's parent is a ConditionalExpression"),
    }
}

/// True when the call `node` (a call to `name` that is a branch of a ternary) is
/// part of the deliberate argument-reversal idiom: somewhere in the enclosing
/// nested-ternary chain a sibling call to `name` passes the same identifier
/// arguments in exactly reversed order (`cond ? f(a, b) : … : f(b, a)`), used to
/// select sort order or normalize which operand comes first — not an accidental
/// swap. A lone reversed call with no reversed sibling is not exempt.
fn in_reversed_arg_ternary<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    name: &str,
) -> bool {
    let nodes = semantic.nodes();

    // The call must itself be a direct branch of a ternary.
    if !matches!(
        nodes.parent_node(node.id()).kind(),
        AstKind::ConditionalExpression(_)
    ) {
        return false;
    }
    let AstKind::CallExpression(this_call) = node.kind() else {
        return false;
    };
    let this_args = arg_names_of_call(this_call);
    if this_args.len() < 2 {
        return false;
    }

    let (chain_start, chain_end) = enclosing_conditional_chain_span(node, semantic);

    // A reversed-argument sibling call to `name` anywhere in the chain marks the
    // reversal as deliberate.
    nodes.iter().any(|other| {
        if other.id() == node.id() {
            return false;
        }
        let AstKind::CallExpression(call) = other.kind() else {
            return false;
        };
        if call.span.start < chain_start || call.span.end > chain_end {
            return false;
        }
        let Expression::Identifier(callee) = &call.callee else {
            return false;
        };
        if callee.name.as_str() != name {
            return false;
        }
        let other_args = arg_names_of_call(call);
        other_args.len() == this_args.len() && this_args.iter().rev().eq(other_args.iter())
    })
}

fn check_call_args(
    call: &oxc_ast::ast::CallExpression,
    func_name: &str,
    params: &[String],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let args = arg_names_of_call(call);

    if let Some(swap) = find_likely_swap(params, &args) {
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "arguments-order".into(),
            message: format!(
                "Argument order may be wrong in `{}()`: '{}' and '{}' appear swapped.",
                func_name, swap.0, swap.1
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Returns (arg1, arg2) if they appear to be swapped based on parameter names.
fn find_likely_swap(params: &[String], args: &[Option<String>]) -> Option<(String, String)> {
    if params.len() < 2 || args.len() < 2 {
        return None;
    }

    for i in 0..params.len().min(args.len()) {
        for j in (i + 1)..params.len().min(args.len()) {
            let Some(arg_i) = &args[i] else {
                continue;
            };
            let Some(arg_j) = &args[j] else {
                continue;
            };

            if names_match(arg_i, &params[j]) && names_match(arg_j, &params[i]) {
                // Names that differ only in case (`n`/`N`, `k`/`K`) are distinct
                // identifiers by mathematical/scientific convention (e.g. sample
                // size `n` vs population size `N`), not an accidental swap.
                if differ_only_in_case(arg_i, arg_j) {
                    continue;
                }
                return Some((arg_i.clone(), arg_j.clone()));
            }
        }
    }
    None
}

fn names_match(arg: &str, param: &str) -> bool {
    let arg_norm = normalize_name(arg);
    let param_norm = normalize_name(param);
    arg_norm == param_norm
}

fn normalize_name(name: &str) -> String {
    name.to_lowercase().trim_start_matches('_').to_string()
}

/// True when `a` and `b` are equal ignoring ASCII case but not exactly equal —
/// i.e. they differ only in the case of some letters.
fn differ_only_in_case(a: &str, b: &str) -> bool {
    a != b && a.eq_ignore_ascii_case(b)
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_reversed_comparator_ternary() {
        // #4411: `cond ? f(a, b) : f(b, a)` is the deliberate sort-order idiom.
        let src = "function compareAscending(a, b) { return 0; }\n\
                   function compareValues(a, b, order) { return order === 'asc' ? compareAscending(a, b) : compareAscending(b, a); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_genuine_swap() {
        let src = "function foo(a, b) {}\nfunction bar(a, b) { foo(b, a); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_ternary_with_single_matching_branch() {
        // Only the alternate calls `cmp`; not a reversed pair, so still flagged.
        let src = "function cmp(a, b) {}\nfunction g(a, b, c) { return c ? cmp(b, a) : other(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_reversed_arg_idiom_across_nested_ternary_chain() {
        // #7273: the un-swapped `isEquivalentArray(a, b)` and the swapped
        // `isEquivalentArray(b, a)` sit at different nesting levels of one ternary
        // chain (`a` is the array → `(a, b)`; `b` is the array → `(b, a)`). The
        // reversed sibling in the chain makes the swap deliberate, not accidental.
        let src = "function isEquivalentArray(a, b) { return true; }\n\
                   function isSame(a, b) { return Array.isArray(a) ? isEquivalentArray(a, b) : Array.isArray(b) ? isEquivalentArray(b, a) : a === b; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_lone_reversed_call_in_ternary_chain() {
        // A reversed call in a nested ternary chain with no un-reversed sibling to
        // the same callee is still an accidental swap.
        let src = "function cmp(a, b) {}\n\
                   function g(a, b, c, d) { return c ? cmp(b, a) : d ? other() : a === b; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_case_only_differing_args() {
        // #6108: lowercase `n` (sample size) and uppercase `N` (population size)
        // are distinct statistical quantities, not an accidental swap.
        let src = "function pdf(n, N) {}\nfunction call(n, N) { pdf(N, n); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_case_only_differing_args_k() {
        // `k`/`K` likewise differ only in case.
        let src = "function dist(k, K) {}\nfunction call(k, K) { dist(K, k); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_genuine_swap_distinct_names() {
        // Same-name reorder where names differ by more than case is still a swap.
        let src = "function foo(width, height) {}\nfunction bar(width, height) { foo(height, width); }";
        assert_eq!(run_on(src).len(), 1);
    }
}
