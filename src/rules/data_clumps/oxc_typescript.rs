//! data-clumps OXC backend.

use rustc_hash::{FxHashMap, FxHashSet};

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

/// Well-known Node.js HTTP middleware signatures (Connect/Express/NestJS), as
/// sorted, deduped parameter-name sets. The framework invokes these callbacks by
/// arity and position, so the group cannot be refactored into a single object
/// parameter — it is a contract, not a data clump.
const FRAMEWORK_MIDDLEWARE_SIGNATURES: &[&[&str]] = &[
    &["next", "req", "res"],          // (req, res, next)
    &["err", "next", "req", "res"],   // (err, req, res, next)
];

/// A 3-param subset is exempt when it is contained in a known middleware
/// signature: every subset of `(req, res, next)` / `(err, req, res, next)` is a
/// framework-mandated group, not a refactorable clump.
fn is_framework_middleware_subset(subset: &[String]) -> bool {
    FRAMEWORK_MIDDLEWARE_SIGNATURES
        .iter()
        .any(|sig| subset.iter().all(|name| sig.contains(&name.as_str())))
}

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
///
/// Underscore-prefixed names (`_`, `__`, `_done`, …) are intentionally-unused
/// positional placeholders: ignore markers with no semantic identity, fixed by
/// the callee's API. They are not data-clump members, so they are dropped.
fn extract_param_names(params: &FormalParameters) -> Vec<String> {
    let mut names = Vec::new();
    for param in &params.items {
        if let BindingPattern::BindingIdentifier(ref id) = param.pattern
            && !id.name.starts_with('_')
        {
            names.push(id.name.to_string());
        }
    }
    if let Some(ref rest) = params.rest
        && let BindingPattern::BindingIdentifier(id) = &rest.rest.argument
        && !id.name.starts_with('_')
    {
        names.push(id.name.to_string());
    }
    names
}

/// Check if a function node is a callback to a framework method like
/// `fastify.register(...)` or a constructor like `new MutationCache({...})`.
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
    // Walk up: function -> ObjectProperty -> ObjectExpression -> CallExpression/NewExpression
    let mut cur = parent_id;
    let mut in_object_expr = false;
    for _ in 0..4 {
        let kind = nodes.kind(cur);
        match kind {
            AstKind::ObjectExpression(_) => {
                in_object_expr = true;
            }
            AstKind::NewExpression(_) => {
                // Constructor calls always impose their callback API on the caller.
                let _ = in_object_expr;
                return true;
            }
            AstKind::CallExpression(call) => {
                let callee_text =
                    &source[call.callee.span().start as usize..call.callee.span().end as usize];
                let method = callee_text.rsplit('.').next().unwrap_or(callee_text);
                return FRAMEWORK_CALLBACK_METHODS.contains(&method);
            }
            _ => {}
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
        let mut subset_occurrences: FxHashMap<Vec<String>, Vec<FnLocation>> = FxHashMap::default();
        for (loc, params) in &fn_params {
            for combo in combinations(params, 3) {
                if is_framework_middleware_subset(&combo) {
                    continue;
                }
                subset_occurrences
                    .entry(combo)
                    .or_default()
                    .push(loc.clone());
            }
        }

        let mut flagged: FxHashSet<FnLocation> = FxHashSet::default();
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
                if let FnLocation::Local(line) = loc
                    && flagged.insert(loc.clone()) {
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn no_fp_on_object_literal_callbacks_in_new_expression() {
        // Regression for issue #751 — MutationCache constructor callbacks share params by library contract.
        let src = r#"
new MutationCache({
  onError(_e, _variables, _context, _mutation) {},
  onSuccess(_d, _variables, _context, _mutation) {},
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_free_standing_functions_sharing_params() {
        let src = r#"
function createUser(name: string, email: string, age: number) {}
function updateUser(name: string, email: string, age: number) {}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_underscore_prefixed_placeholder_params() {
        // Regression for issue #3857 — `_` and `_done` are intentionally-unused
        // positional placeholders; after dropping them only `body` remains, below
        // the clump threshold, so the pair must not be flagged.
        let src = r#"
function f1(_: unknown, body: string, _done: () => void) {}
function f2(_: unknown, body: string, _done: () => void) {}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_real_params_when_placeholders_dropped() {
        // After dropping `_`, three semantically-named params remain in both
        // functions, so the clump must still flag and the message must list only
        // the real params.
        let src = r#"
function f1(_: unknown, userId: string, name: string, email: string) {}
function f2(_: unknown, userId: string, name: string, email: string) {}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("email, name, userId"));
        assert!(!diags[0].message.contains('_'));
    }

    #[test]
    fn no_fp_on_express_nest_middleware_signature() {
        // Regression for issue #2035 — (req, res, next) is the Node.js HTTP
        // middleware contract, invoked by arity/position, not a data clump.
        let src = r#"
export class AppModule {
  configure(consumer: MiddlewareConsumer) {
    consumer
      .apply((req, res, next) => res.end(MIDDLEWARE_VALUE))
      .forRoutes({ path: MIDDLEWARE_VALUE })
      .apply((req, res, next) => res.status(201).end(MIDDLEWARE_VALUE))
      .forRoutes({ path: MIDDLEWARE_VALUE })
      .apply((req, res, next) => res.end(MIDDLEWARE_PARAM_VALUE))
      .forRoutes({ path: MIDDLEWARE_VALUE });
  }
}
function logger(req, res, next) { next(); }
function auth(req, res, next) { next(); }
function errorHandler(err, req, res, next) { next(err); }
"#;
        assert!(run(src).is_empty());
    }
}
