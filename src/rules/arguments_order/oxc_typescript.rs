//! OXC backend for arguments-order.
//!
//! Flags call sites whose identifier arguments match the callee's parameter
//! names in reversed order (a likely accidental swap). A call is exempt when it
//! is a branch of a ternary whose consequent and alternate both call the same
//! function with argument lists that are exact reverses of each other
//! (`cond ? f(a, b) : f(b, a)`) — the deliberate argument-reversal idiom for
//! selecting sort order, not an accidental swap.

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

/// Names of the identifier arguments of `expr` if it is a `CallExpression` whose
/// callee is the bare identifier `name`; otherwise `None`. Non-identifier args
/// map to a `None` slot so positions still line up.
fn call_arg_names_if_to<'a>(expr: &Expression<'a>, name: &str) -> Option<Vec<Option<String>>> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name.as_str() != name {
        return None;
    }
    Some(
        call.arguments
            .iter()
            .map(|a| match a {
                oxc_ast::ast::Argument::Identifier(id) => Some(id.name.to_string()),
                _ => None,
            })
            .collect(),
    )
}

/// True when `node` (a `CallExpression` to `name`) is a branch of a ternary whose
/// consequent and alternate both call `name` with argument lists that are exact
/// reverses of each other (`cond ? f(a, b) : f(b, a)`) — the deliberate
/// argument-reversal idiom for selecting sort order, not an accidental swap.
fn in_reversed_arg_ternary<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    name: &str,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    let AstKind::ConditionalExpression(cond) = parent.kind() else {
        return false;
    };
    let (Some(consequent), Some(alternate)) = (
        call_arg_names_if_to(&cond.consequent, name),
        call_arg_names_if_to(&cond.alternate, name),
    ) else {
        return false;
    };
    consequent.len() >= 2
        && consequent.len() == alternate.len()
        && consequent.iter().rev().eq(alternate.iter())
}

fn check_call_args(
    call: &oxc_ast::ast::CallExpression,
    func_name: &str,
    params: &[String],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut args: Vec<Option<String>> = Vec::new();
    for arg in &call.arguments {
        match arg {
            oxc_ast::ast::Argument::Identifier(id) => {
                args.push(Some(id.name.to_string()));
            }
            _ => {
                args.push(None);
            }
        }
    }

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
