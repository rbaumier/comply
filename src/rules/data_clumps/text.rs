use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct Check;

/// Extract parameter names from a function signature line.
/// Looks for content between `(` and `)` and splits by `,`.
fn extract_param_names(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let open = match trimmed.find('(') {
        Some(p) => p,
        None => return Vec::new(),
    };
    let close = match trimmed[open..].find(')') {
        Some(p) => open + p,
        None => return Vec::new(),
    };
    let params_str = &trimmed[open + 1..close];
    params_str
        .split(',')
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() {
                return None;
            }
            // Take just the name (before `:` or `=` or `?`)
            let name = p
                .split(&[':', '=', '?'][..])
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches("...");
            // Strip modifiers like readonly, public, private, protected
            let name = name
                .split_whitespace()
                .last()
                .unwrap_or("");
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

/// Check if a line looks like a function declaration.
fn is_function_sig(line: &str) -> bool {
    let t = line.trim();
    t.contains("function ") || (t.contains('(') && (t.contains("=>") || t.contains(") {") || t.contains("): ")))
}

/// Generate all subsets of size `k` from a sorted slice (combinations).
fn combinations(items: &[String], k: usize) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    let mut combo = vec![0usize; k];
    fn recurse(items: &[String], k: usize, start: usize, combo: &mut Vec<usize>, depth: usize, result: &mut Vec<Vec<String>>) {
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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Collect parameter sets per function, with line numbers.
        let mut fn_params: Vec<(usize, Vec<String>)> = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            if is_function_sig(line) {
                let mut params = extract_param_names(line);
                if params.len() >= 3 {
                    params.sort();
                    params.dedup();
                    if params.len() >= 3 {
                        fn_params.push((idx + 1, params));
                    }
                }
            }
        }

        // For each 3-param subset, count how many functions contain it.
        let mut subset_occurrences: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
        for (line, params) in &fn_params {
            let seen: HashSet<Vec<String>> = HashSet::new();
            let _ = seen; // suppress warning
            for combo in combinations(params, 3) {
                subset_occurrences.entry(combo).or_default().push(*line);
            }
        }

        let mut diagnostics = Vec::new();
        let mut flagged_lines: HashSet<usize> = HashSet::new();

        for (subset, lines) in &subset_occurrences {
            if lines.len() >= 2 {
                for &line in lines {
                    if flagged_lines.insert(line) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line,
                            column: 1,
                            rule_id: "data-clumps".into(),
                            message: format!(
                                "Parameters [{}] appear together in {} functions — extract into a value object.",
                                subset.join(", "),
                                lines.len(),
                            ),
                            severity: Severity::Warning,
                        });
                    }
                }
            }
        }
        diagnostics.sort_by_key(|d| d.line);
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_repeated_param_group() {
        let src = r#"
function createUser(name: string, email: string, age: number) {}
function updateUser(name: string, email: string, age: number) {}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_different_params() {
        let src = r#"
function createUser(name: string, email: string, age: number) {}
function sendEmail(to: string, subject: string, body: string) {}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_fewer_than_three_shared() {
        let src = r#"
function foo(a: string, b: string, c: number) {}
function bar(a: string, b: string, d: number) {}
"#;
        assert!(run(src).is_empty());
    }
}
