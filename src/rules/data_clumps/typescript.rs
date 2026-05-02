//! data-clumps — flag functions sharing 3+ identical parameter names.
//!
//! Walks the AST to find function-like nodes, extracts their parameter
//! names, and flags when the same 3-parameter subset appears in 2+ functions.
//!
//! Cross-file: also considers exported functions from imported modules via
//! ImportIndex, detecting clumps across file boundaries.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Node kinds that can have `parameters` / `formal_parameters`.
const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function_declaration",
    "generator_function",
];

const FRAMEWORK_CALLBACK_METHODS: &[&str] = &[
    "register", "addHook", "route", "get", "post", "put", "patch", "delete", "head", "options",
    "all",
];

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum FnLocation {
    Local(usize),
    External(PathBuf, String, usize),
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
        return;
    }

    let mut fn_params: Vec<(FnLocation, Vec<String>)> = Vec::new();
    collect_functions(node, source, &mut fn_params);

    // Add exported functions from imported modules (cross-file)
    let index = ctx.project.import_index();
    for imp in index.get_imports(ctx.path) {
        let Some(src_path) = &imp.source_path else { continue; };
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
            subset_occurrences.entry(combo).or_default().push(loc.clone());
        }
    }

    let mut flagged: HashSet<FnLocation> = HashSet::new();
    let mut results: Vec<(usize, String)> = Vec::new();

    for (subset, locations) in &subset_occurrences {
        if locations.len() < 2 { continue; }

        // Collect external function info for the message
        let external_locs: Vec<_> = locations.iter()
            .filter_map(|l| match l {
                FnLocation::External(path, name, _) => Some((path, name)),
                _ => None,
            })
            .collect();

        // Only flag local functions (we can't fix external ones)
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
                        let ext_names: Vec<_> = external_locs.iter()
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
    for (line, message) in results {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: "data-clumps".into(),
            message,
            severity: Severity::Warning,
            span: None,
        });
    }
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

/// Recursively collect function parameter sets from the AST.
fn collect_functions(
    node: tree_sitter::Node,
    source: &[u8],
    out: &mut Vec<(FnLocation, Vec<String>)>,
) {
    if FUNCTION_KINDS.contains(&node.kind()) {
        if is_framework_callback(node, source) {
            return;
        }

        let params_node = node
            .child_by_field_name("parameters")
            .or_else(|| node.child_by_field_name("formal_parameters"));

        if let Some(params) = params_node {
            let mut names: Vec<String> = Vec::new();
            let count = params.named_child_count();
            for i in 0..count {
                if let Some(param) = params.named_child(i)
                    && let Some(name) = extract_param_name(param, source)
                {
                    names.push(name);
                }
            }
            names.sort();
            names.dedup();
            if names.len() >= 3 {
                out.push((FnLocation::Local(node.start_position().row + 1), names));
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_functions(cursor.node(), source, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn is_framework_callback(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "arguments" {
        return false;
    }
    let Some(call) = parent.parent() else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    callee_name(callee, source).is_some_and(|name| FRAMEWORK_CALLBACK_METHODS.contains(&name))
}

fn callee_name<'a>(callee: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match callee.kind() {
        "identifier" => callee.utf8_text(source).ok(),
        "member_expression" => callee
            .child_by_field_name("property")?
            .utf8_text(source)
            .ok(),
        _ => None,
    }
}

/// Extract the binding name from a formal parameter node.
fn extract_param_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok().map(String::from),
        "required_parameter" | "optional_parameter" => {
            let pat = node.child_by_field_name("pattern")?;
            pat.utf8_text(source).ok().map(String::from)
        }
        "rest_pattern" => {
            let child = node.named_child(0)?;
            child.utf8_text(source).ok().map(String::from)
        }
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_repeated_param_group() {
        let src = r#"
function createUser(name: string, email: string, age: number) {}
function updateUser(name: string, email: string, age: number) {}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_fastify_register_callback_signature() {
        let src = r#"
fastify.register((instance, opts, done) => {
  done();
});
app.register((instance, opts, done) => {
  done();
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_repeated_param_groups_in_test_files() {
        let src = r#"
function arrangeUser(req: Request, reply: Reply, done: Done) {}
function arrangeAccount(req: Request, reply: Reply, done: Done) {}
"#;
        assert!(run_on_path(src, "plugin.test.ts").is_empty());
    }

    #[test]
    fn allows_different_params() {
        let src = r#"
function createUser(name: string, email: string, age: number) {}
function sendEmail(to: string, subject: string, body: string) {}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fewer_than_three_shared() {
        let src = r#"
function foo(a: string, b: string, c: number) {}
function bar(a: string, b: string, d: number) {}
"#;
        assert!(run_on(src).is_empty());
    }

    // Cross-file tests would require multi-file setup via ProjectCtx
}
