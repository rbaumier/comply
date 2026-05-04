//! data-clumps OXC backend.

use std::collections::{HashMap, HashSet};

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const FRAMEWORK_CALLBACK_METHODS: &[&str] = &[
    "register", "addHook", "route", "get", "post", "put", "patch", "delete", "head", "options",
    "all",
];

fn is_test_path(path: &std::path::Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/");
    lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("/fixtures/")
        || lower.contains("/__tests__/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum FnLocation {
    Local(usize),
    External(std::path::PathBuf, String, usize),
}

/// Extract parameter names from a function's formal parameters.
fn extract_param_names(params: &FormalParameters) -> Vec<String> {
    let mut names = Vec::new();
    for param in &params.items {
        if let BindingPattern::BindingIdentifier(ref id) = param.pattern {
            names.push(id.name.to_string());
        }
    }
    if let Some(ref rest) = params.rest {
        if let BindingPattern::BindingIdentifier(id) = &rest.rest.argument {
            names.push(id.name.to_string());
        }
    }
    names
}

/// Check if a function node is a callback to a framework method like
/// `fastify.register(...)`.
fn is_framework_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    // The function should be an argument of a call expression.
    // Walk up: function -> argument (Argument) -> CallExpression
    let mut cur = parent_id;
    for _ in 0..3 {
        let kind = nodes.kind(cur);
        if let AstKind::CallExpression(call) = kind {
            let callee_text =
                &source[call.callee.span().start as usize..call.callee.span().end as usize];
            let method = callee_text.rsplit('.').next().unwrap_or(callee_text);
            return FRAMEWORK_CALLBACK_METHODS.contains(&method);
        }
        let next = nodes.parent_id(cur);
        if next == cur {
            break;
        }
        cur = next;
    }
    false
}

/// Generate all sorted subsets of size `k` from `items`.
fn combinations(items: &[String], k: usize) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    let mut combo = vec![0usize; k];
    fn recurse(
        items: &[String],
        k: usize,
        start: usize,
        combo: &mut Vec<usize>,
        depth: usize,
        result: &mut Vec<Vec<String>>,
    ) {
        if depth == k {
            result.push(combo[..k].iter().map(|&i| items[i].clone()).collect());
            return;
        }
        if start + (k - depth) > items.len() {
            return;
        }
        for i in start..items.len() {
            combo[depth] = i;
            recurse(items, k, i + 1, combo, depth + 1, result);
        }
    }
    recurse(items, k, 0, &mut combo, 0, &mut result);
    result
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return Vec::new();
        }

        let nodes = semantic.nodes();
        let mut fn_params: Vec<(FnLocation, Vec<String>)> = Vec::new();

        // Collect function parameter sets from the AST
        for node in nodes.iter() {
            let params = match node.kind() {
                AstKind::Function(func) => {
                    if is_framework_callback(node, semantic, ctx.source) {
                        continue;
                    }
                    Some(&func.params)
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if is_framework_callback(node, semantic, ctx.source) {
                        continue;
                    }
                    Some(&arrow.params)
                }
                _ => None,
            };

            if let Some(params) = params {
                let mut names = extract_param_names(params);
                names.sort();
                names.dedup();
                if names.len() >= 3 {
                    let span = node.kind().span();
                    let line = crate::oxc_helpers::byte_offset_to_line_col(
                        ctx.source,
                        span.start as usize,
                    )
                    .0;
                    fn_params.push((FnLocation::Local(line), names));
                }
            }
        }

        // Add exported functions from imported modules (cross-file)
        let index = ctx.project.import_index();
        for imp in index.get_imports(ctx.path) {
            let Some(src_path) = &imp.source_path else {
                continue;
            };
            for export in index.get_exports(src_path) {
                if export.params.len() >= 3 {
                    let mut sorted_params = export.params.clone();
                    sorted_params.sort();
                    sorted_params.dedup();
                    if sorted_params.len() >= 3 {
                        fn_params.push((
                            FnLocation::External(src_path.clone(), export.name.clone(), export.line),
                            sorted_params,
                        ));
                    }
                }
            }
        }

        // For each 3-param subset, count which functions contain it.
        let mut subset_occurrences: HashMap<Vec<String>, Vec<FnLocation>> = HashMap::new();
        for (loc, params) in &fn_params {
            for combo in combinations(params, 3) {
                subset_occurrences
                    .entry(combo)
                    .or_default()
                    .push(loc.clone());
            }
        }

        let mut flagged: HashSet<FnLocation> = HashSet::new();
        let mut results: Vec<(usize, String)> = Vec::new();

        for (subset, locations) in &subset_occurrences {
            if locations.len() < 2 {
                continue;
            }

            let external_locs: Vec<_> = locations
                .iter()
                .filter_map(|l| match l {
                    FnLocation::External(path, name, _) => Some((path, name)),
                    _ => None,
                })
                .collect();

            for loc in locations {
                if let FnLocation::Local(line) = loc {
                    if flagged.insert(loc.clone()) {
                        let msg = if external_locs.is_empty() {
                            format!(
                                "Parameters [{}] appear together in {} functions — extract into a type.",
                                subset.join(", "),
                                locations.len(),
                            )
                        } else {
                            let ext_names: Vec<_> = external_locs
                                .iter()
                                .map(|(_, name)| name.as_str())
                                .collect();
                            format!(
                                "Parameters [{}] also used by imported function(s): {} — extract into a shared type.",
                                subset.join(", "),
                                ext_names.join(", "),
                            )
                        };
                        results.push((*line, msg));
                    }
                }
            }
        }

        results.sort_by_key(|(line, _)| *line);
        results
            .into_iter()
            .map(|(line, message)| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message,
                severity: Severity::Warning,
                span: None,
            })
            .collect()
    }
}
