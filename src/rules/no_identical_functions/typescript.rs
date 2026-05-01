//! no-identical-functions backend — flag functions with identical bodies.
//!
//! Two detection paths run inside the same AST walk:
//!
//! - **Intra-file**: classic pairwise comparison of functions declared in
//!   the same program. Kept as the sole path when the per-run
//!   `ImportIndex` is empty (LSP in-editor buffers, unit tests that
//!   construct `CheckCtx::for_test` — they have no multi-file view).
//!
//! - **Cross-file**: when the `ImportIndex` is populated, a process-wide
//!   cache keyed by the index's pointer identity collects every function
//!   body across every indexed TS/JS/TSX file exactly once. Re-parsing
//!   happens inside this rule (the `ImportIndex` doesn't retain ASTs and
//!   extending it for a single consumer wasn't worth it). Per-file
//!   dispatch then reports the duplicate groups that include functions
//!   declared in `ctx.path`, listing every participant site.
//!
//! Normalization is whitespace-only: tokens are unchanged, so `foo`
//! renamed to `bar` is NOT treated as a duplicate. Alpha-renaming could
//! be bolted on later if the false-negative rate is high.
//!
//! Thresholds (both must hold): body must have more than three lines AND
//! more than fifty normalized characters. Trivial getters / delegation
//! stubs slip under the floor and don't get flagged.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tree_sitter::{Node, Parser};

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::ImportIndex;

/// One function participating in a duplicate group. Stored in the
/// process-wide cross-file index so every file in the group can reference
/// every other.
#[derive(Debug, Clone)]
struct FunctionLocation {
    path: PathBuf,
    line: usize,
    name: String,
}

/// Collapse runs of whitespace per line and drop blank lines. Keeps every
/// non-whitespace token — variable renames still defeat the hash.
fn normalize_body(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn body_meets_threshold(raw: &str, normalized: &str, min_body_lines: usize, min_normalized_chars: usize) -> bool {
    raw.lines().count() >= min_body_lines && normalized.len() >= min_normalized_chars
}

fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Process-wide cache: maps an `ImportIndex` pointer identity to the
/// (body-hash → locations) map built by re-parsing every indexed file.
///
/// The pointer key is stable within one `comply` invocation — `ProjectCtx`
/// is built once and the `ImportIndex` lives inside it. When a new run
/// constructs a different `ImportIndex`, we rebuild. Tests call with the
/// shared default empty index (which `is_empty()` detects), never reaching
/// this cache.
#[allow(clippy::type_complexity)]
static CROSS_FILE_CACHE: OnceLock<
    Mutex<HashMap<usize, std::sync::Arc<HashMap<u64, Vec<FunctionLocation>>>>>,
> = OnceLock::new();

#[allow(clippy::type_complexity)]
fn cross_file_cache()
-> &'static Mutex<HashMap<usize, std::sync::Arc<HashMap<u64, Vec<FunctionLocation>>>>> {
    CROSS_FILE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Build (or fetch from cache) the process-wide hash of every function
/// body across the indexed file set. Re-parses each file with its own
/// local parser — `tree_sitter::Parser` is `!Sync`, so we can't reuse
/// the engine's parser across a flat fan-out without a bigger refactor.
fn cross_file_index(index: &ImportIndex, min_body_lines: usize, min_normalized_chars: usize) -> std::sync::Arc<HashMap<u64, Vec<FunctionLocation>>> {
    let key = std::ptr::from_ref::<ImportIndex>(index) as usize;
    let mut cache = cross_file_cache()
        .lock()
        .expect("cross-file cache poisoned");
    if let Some(hit) = cache.get(&key) {
        return std::sync::Arc::clone(hit);
    }
    let built = build_cross_file_index(index, min_body_lines, min_normalized_chars);
    let arc = std::sync::Arc::new(built);
    cache.insert(key, std::sync::Arc::clone(&arc));
    arc
}

fn build_cross_file_index(index: &ImportIndex, min_body_lines: usize, min_normalized_chars: usize) -> HashMap<u64, Vec<FunctionLocation>> {
    let mut by_hash: HashMap<u64, Vec<FunctionLocation>> = HashMap::new();
    let mut parser = Parser::new();
    for path in index.indexed_paths() {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let grammar: tree_sitter::Language = if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tsx") || ext.eq_ignore_ascii_case("jsx"))
        {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };
        if parser.set_language(&grammar).is_err() {
            continue;
        }
        let Some(tree) = parser.parse(source.as_bytes(), None) else {
            continue;
        };
        let mut collected: Vec<(String, usize, String)> = Vec::new();
        let root = tree.root_node();
        let count = root.named_child_count();
        for i in 0..count {
            let Some(child) = root.named_child(i) else {
                continue;
            };
            collect_functions(child, source.as_bytes(), &mut collected, min_body_lines, min_normalized_chars);
        }
        for (name, line, normalized) in collected {
            let h = hash_str(&normalized);
            by_hash.entry(h).or_default().push(FunctionLocation {
                path: path.to_path_buf(),
                line,
                name,
            });
        }
    }
    // Prune non-duplicates so lookups touch only interesting buckets.
    by_hash.retain(|_, v| v.len() > 1);
    by_hash
}

/// Per-file collector used by both the in-file path and the cross-file
/// index builder. Walks function declarations and `const x = () => { … }`
/// / `const x = function(){}` bindings, applies the two thresholds, and
/// pushes `(name, line, normalized_body)` triples.
fn collect_functions(node: Node, source: &[u8], out: &mut Vec<(String, usize, String)>, min_body_lines: usize, min_normalized_chars: usize) {
    match node.kind() {
        "function_declaration" => {
            if let Some((name, line, body)) = extract_function_info(node, source) {
                let normalized = normalize_body(&body);
                if body_meets_threshold(&body, &normalized, min_body_lines, min_normalized_chars) {
                    out.push((name, line, normalized));
                }
            }
        }
        "lexical_declaration" => {
            // `const foo = () => { … }` or `const foo = function(){ … }`
            let count = node.named_child_count();
            for i in 0..count {
                let Some(declarator) = node.named_child(i) else {
                    continue;
                };
                if declarator.kind() != "variable_declarator" {
                    continue;
                }
                let Some(name_node) = declarator.child_by_field_name("name") else {
                    continue;
                };
                let Ok(name) = name_node.utf8_text(source) else {
                    continue;
                };
                let Some(value) = declarator.child_by_field_name("value") else {
                    continue;
                };
                let body_node = match value.kind() {
                    "arrow_function" | "function" => value.child_by_field_name("body"),
                    _ => None,
                };
                if let Some(body_n) = body_node
                    && let Ok(body_text) = body_n.utf8_text(source)
                {
                    let normalized = normalize_body(body_text);
                    if body_meets_threshold(body_text, &normalized, min_body_lines, min_normalized_chars) {
                        let line = name_node.start_position().row + 1;
                        out.push((name.to_string(), line, normalized));
                    }
                }
            }
        }
        "export_statement" => {
            // Recurse into `export function foo()` / `export const foo = …`.
            let count = node.named_child_count();
            for i in 0..count {
                if let Some(child) = node.named_child(i) {
                    collect_functions(child, source, out, min_body_lines, min_normalized_chars);
                }
            }
        }
        _ => {}
    }
}

fn extract_function_info(node: Node, source: &[u8]) -> Option<(String, usize, String)> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;
    let body_node = node.child_by_field_name("body")?;
    let body = body_node.utf8_text(source).ok()?;
    let line = name_node.start_position().row + 1;
    Some((name.to_string(), line, body.to_string()))
}

/// Render the "duplicate group" message shared by every site. Marks the
/// current file's entry with `[this file]` so the user knows which line
/// the diagnostic is anchored on.
fn format_group_message(
    group: &[FunctionLocation],
    current_file: &Path,
    current_line: usize,
) -> String {
    let mut msg = format!("Duplicate function body ({} occurrences):", group.len());
    for loc in group {
        let marker = if loc.path == current_file && loc.line == current_line {
            "  [this file]"
        } else {
            ""
        };
        msg.push_str(&format!(
            "\n  - {}:{} `{}`{}",
            loc.path.display(),
            loc.line,
            loc.name,
            marker,
        ));
    }
    msg.push_str("\nExtract the duplicated logic into a shared helper.");
    msg
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let import_index = ctx.project.import_index();
    let min_body_lines = ctx.config.threshold("no-identical-functions", "min_body_lines", ctx.lang);
    let min_normalized_chars = ctx.config.threshold("no-identical-functions", "min_normalized_chars", ctx.lang);

    // Collect this file's functions once — both paths reuse the same list.
    let mut local_functions: Vec<(String, usize, String)> = Vec::new();
    let child_count = node.named_child_count();
    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        collect_functions(child, source, &mut local_functions, min_body_lines, min_normalized_chars);
    }

    if import_index.is_empty() {
        // Intra-file-only path: flag the first pair per match, same as the
        // pre-cross-file behaviour so test expectations stay stable.
        for i in 1..local_functions.len() {
            for j in 0..i {
                if local_functions[i].2 == local_functions[j].2 {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: local_functions[i].1,
                        column: 1,
                        rule_id: "no-identical-functions".into(),
                        message: format!(
                            "Function `{}` has an identical body to `{}` (line {}). Extract the duplicated logic into a shared helper.",
                            local_functions[i].0,
                            local_functions[j].0,
                            local_functions[j].1,
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
        }
        return;
    }

    // Cross-file path: look up every local function's body hash in the
    // global duplicate map. Emit one diagnostic per local function that
    // belongs to a duplicate group, anchored on its line, messaging the
    // full group. De-duplicate on `(hash, line)` so a function that
    // appears twice in the group (shouldn't, but guard anyway) doesn't
    // fire twice.
    let global = cross_file_index(import_index, min_body_lines, min_normalized_chars);
    let mut fired: HashSet<(u64, usize)> = HashSet::new();
    for (_name, line, normalized) in &local_functions {
        let h = hash_str(normalized);
        let Some(group) = global.get(&h) else { continue };
        if group.len() < 2 { continue }
        if !fired.insert((h, *line)) { continue }
        let message = format_group_message(group, ctx.path, *line);
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: *line,
            column: 1,
            rule_id: "no-identical-functions".into(),
            message,
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_identical_functions() {
        let src = r#"
function foo(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}

function bar(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bar"));
        assert!(d[0].message.contains("foo"));
    }

    #[test]
    fn allows_different_functions() {
        let src = r#"
function foo(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}

function bar(x: number) {
    const a = x - 1;
    const b = a / 2;
    console.log(b);
    return b;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_short_identical_bodies() {
        let src = r#"
function foo() {
    return 1;
}

function bar() {
    return 1;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_identical_bodies_below_char_threshold() {
        // Four lines but trivially short once normalized — below the
        // 51-char normalized floor.
        let src = r#"
function foo() {
    let a = 1;
    let b = 2;
    return a;
}

function bar() {
    let a = 1;
    let b = 2;
    return a;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
