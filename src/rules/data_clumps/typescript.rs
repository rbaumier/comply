//! data-clumps — flag functions sharing 3+ identical parameter names.
//!
//! Walks the AST to find function-like nodes, extracts their parameter
//! names, and flags when the same 3-parameter subset appears in 2+ functions.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::{HashMap, HashSet};

/// Node kinds that can have `parameters` / `formal_parameters`.
const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function_declaration",
    "generator_function",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    // We process the entire tree at the program level to collect all
    // function signatures in one pass.
    if node.kind() != "program" {
        return;
    }

    let mut fn_params: Vec<(usize, Vec<String>)> = Vec::new();
    collect_functions(node, source, &mut fn_params);

    // For each 3-param subset, count how many functions contain it.
    let mut subset_occurrences: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
    for (line, params) in &fn_params {
        for combo in combinations(params, 3) {
            subset_occurrences.entry(combo).or_default().push(*line);
        }
    }

    let mut flagged_lines: HashSet<usize> = HashSet::new();
    let mut results: Vec<(usize, String)> = Vec::new();

    for (subset, lines) in &subset_occurrences {
        if lines.len() >= 2 {
            for &line in lines {
                if flagged_lines.insert(line) {
                    results.push((
                        line,
                        format!(
                            "Parameters [{}] appear together in {} functions \
                             — extract into a value object.",
                            subset.join(", "),
                            lines.len(),
                        ),
                    ));
                }
            }
        }
    }

    results.sort_by_key(|(line, _)| *line);
    for (line, message) in results {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line,
            column: 1,
            rule_id: "data-clumps".into(),
            message,
            severity: Severity::Warning,
        });
    }
}

/// Recursively collect function parameter sets from the AST.
fn collect_functions(
    node: tree_sitter::Node,
    source: &[u8],
    out: &mut Vec<(usize, Vec<String>)>,
) {
    if FUNCTION_KINDS.contains(&node.kind()) {
        let params_node = node
            .child_by_field_name("parameters")
            .or_else(|| node.child_by_field_name("formal_parameters"));

        if let Some(params) = params_node {
            let mut names: Vec<String> = Vec::new();
            let count = params.named_child_count();
            for i in 0..count {
                if let Some(param) = params.named_child(i)
                    && let Some(name) = extract_param_name(param, source) {
                        names.push(name);
                    }
            }
            names.sort();
            names.dedup();
            if names.len() >= 3 {
                out.push((node.start_position().row + 1, names));
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

/// Extract the binding name from a formal parameter node.
fn extract_param_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok().map(String::from),
        "required_parameter" | "optional_parameter" => {
            // The `pattern` field holds the identifier.
            let pat = node.child_by_field_name("pattern")?;
            pat.utf8_text(source).ok().map(String::from)
        }
        "rest_pattern" => {
            // `...name`
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

    #[test]
    fn flags_repeated_param_group() {
        let src = r#"
function createUser(name: string, email: string, age: number) {}
function updateUser(name: string, email: string, age: number) {}
"#;
        assert_eq!(run_on(src).len(), 2);
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
}
