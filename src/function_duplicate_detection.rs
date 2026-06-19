//! Cross-file exact-duplicate named-function detection.
//!
//! A named function copy-pasted verbatim into another file is duplication that
//! `no-clones` misses: its body is shorter than the 100-token window clone
//! detection requires. The shared *name* is a strong enough signal to drop the
//! length requirement — two functions with the same identifier and a
//! byte-identical body (modulo comments and whitespace) across files are a
//! copy-paste that belongs in one shared module.
//!
//! - Extract every named function with a body from each TS/JS file:
//!   `function foo() {…}` declarations and `const foo = (…) => {…}` /
//!   `const foo = function (…) {…}` bindings.
//! - Tokenize the body (leaf tokens, comments excluded) into an exact
//!   signature, so formatting and comments do not matter but renamed
//!   identifiers do.
//! - Bucket by `(name, signature)`; a bucket spanning two or more files whose
//!   body clears `min_body_tokens` is reported, one diagnostic per extra file.

use rustc_hash::{FxHashMap, FxHashSet};

use rayon::prelude::*;
use tree_sitter::{Node, Parser};

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Severity};
use crate::files::{Language, SourceFile};
use crate::parsing::parse_with_grammar;

pub const RULE_ID: &str = "no-duplicate-function";

fn is_target_language(lang: Language) -> bool {
    matches!(lang, Language::TypeScript | Language::Tsx | Language::JavaScript)
}

fn is_comment_kind(kind: &str) -> bool {
    matches!(kind, "comment" | "line_comment" | "block_comment")
}

/// A named function eligible to be compared against others.
struct FnEntry {
    file_idx: usize,
    name: String,
    line: usize,
    column: usize,
    span: (usize, usize),
    /// Exact body fingerprint: each leaf token's `(kind_id, text)`, in order.
    /// Two bodies are duplicates iff their signatures are byte-equal.
    signature: Vec<u8>,
}

#[must_use]
pub fn lint_files(files: &[&SourceFile], config: &Config) -> Vec<Diagnostic> {
    // Sample/example/docs/fixture dirs hold intentionally self-contained,
    // duplicated code; generated files are machine-emitted. Drop both so a
    // relaxed file is neither reported nor used as a canonical match.
    let files: Vec<&SourceFile> = files
        .iter()
        .copied()
        .filter(|f| is_target_language(f.language))
        .filter(|f| {
            !crate::rules::file_ctx::scan_path(&f.path).is_relaxed_dir
                && !crate::rules::file_ctx::is_generated_path(&f.path)
        })
        .collect();

    if files.len() < 2 {
        return vec![];
    }

    // Cross-language rule: its single knob lives in a non-per-language
    // `[rules.<id>]` block, so the `Language` passed to the lookup is immaterial.
    let min_body_tokens = config.threshold(RULE_ID, "min_body_tokens", Language::TypeScript);

    let entries: Vec<FnEntry> = files
        .par_iter()
        .enumerate()
        .map_init(Parser::new, |parser, (idx, file)| {
            extract_functions(parser, file, idx, min_body_tokens)
        })
        .flatten()
        .collect();

    let mut buckets: FxHashMap<(&str, &[u8]), Vec<usize>> = FxHashMap::default();
    for (i, entry) in entries.iter().enumerate() {
        buckets
            .entry((entry.name.as_str(), entry.signature.as_slice()))
            .or_default()
            .push(i);
    }

    let mut diags = Vec::new();
    for members in buckets.values() {
        if members.len() < 2 {
            continue;
        }
        // Inter-file only: collapse to one representative per file (the earliest
        // by line), in path order. A name+body repeated within a single file is
        // out of scope. Fewer than two distinct files → nothing to report.
        let mut ordered = members.clone();
        ordered.sort_by(|&a, &b| {
            let (ea, eb) = (&entries[a], &entries[b]);
            files[ea.file_idx]
                .path
                .cmp(&files[eb.file_idx].path)
                .then(ea.line.cmp(&eb.line))
        });
        let mut seen_files: FxHashSet<usize> = FxHashSet::default();
        let reps: Vec<usize> = ordered
            .into_iter()
            .filter(|&i| seen_files.insert(entries[i].file_idx))
            .collect();
        if reps.len() < 2 {
            continue;
        }

        // The lexicographically-first file is canonical; every other file
        // reports once, pointing at it — so N files yield N-1 diagnostics.
        let canonical = &entries[reps[0]];
        for &m in reps.iter().skip(1) {
            let entry = &entries[m];
            diags.push(Diagnostic {
                path: std::sync::Arc::from(files[entry.file_idx].path.as_path()),
                line: entry.line,
                column: entry.column,
                rule_id: RULE_ID.into(),
                message: format!(
                    "Duplicate function `{}` — an identical definition is in `{}` at line {}. \
                     Extract it into a shared module and import it from both call sites.",
                    entry.name,
                    files[canonical.file_idx].path.display(),
                    canonical.line,
                ),
                severity: Severity::Warning,
                span: Some(entry.span),
            });
        }
    }

    diags.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    diags
}

fn extract_functions(
    parser: &mut Parser,
    file: &SourceFile,
    file_idx: usize,
    min_body_tokens: usize,
) -> Vec<FnEntry> {
    let Ok(source) = std::fs::read_to_string(&file.path) else {
        return Vec::new();
    };
    if crate::rules::file_ctx::is_generated_content(&source) {
        return Vec::new();
    }
    let Some(tree) = parse_with_grammar(parser, file.language, source.as_bytes()) else {
        return Vec::new();
    };
    let bytes = source.as_bytes();

    let mut entries = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if let Some((name, decl, body)) = named_function_parts(node) {
            if let Some(entry) =
                build_entry(name, decl, body, bytes, file_idx, min_body_tokens)
            {
                entries.push(entry);
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return entries;
            }
        }
    }
}

/// The `(name, declaration, body)` of a named function at `node`, for the two
/// forms in scope: a `function foo() {…}` declaration, and a `const foo = …`
/// binding whose value is an arrow or function expression. Overload signatures
/// and ambient declarations carry no `body` field and so are skipped.
fn named_function_parts<'a>(node: Node<'a>) -> Option<(Node<'a>, Node<'a>, Node<'a>)> {
    match node.kind() {
        "function_declaration" => {
            let name = node.child_by_field_name("name")?;
            let body = node.child_by_field_name("body")?;
            Some((name, node, body))
        }
        "variable_declarator" => {
            let name = node.child_by_field_name("name")?;
            if name.kind() != "identifier" {
                return None;
            }
            let value = node.child_by_field_name("value")?;
            if !matches!(value.kind(), "arrow_function" | "function_expression") {
                return None;
            }
            let body = value.child_by_field_name("body")?;
            Some((name, node, body))
        }
        _ => None,
    }
}

fn build_entry(
    name: Node,
    decl: Node,
    body: Node,
    source: &[u8],
    file_idx: usize,
    min_body_tokens: usize,
) -> Option<FnEntry> {
    let name_text = source.get(name.start_byte()..name.end_byte())?;
    let name_str = std::str::from_utf8(name_text).ok()?.to_string();

    let mut signature = Vec::new();
    let mut token_count = 0;
    collect_body_tokens(body, source, &mut signature, &mut token_count);
    if token_count < min_body_tokens {
        return None;
    }

    let pos = name.start_position();
    Some(FnEntry {
        file_idx,
        name: name_str,
        line: pos.row + 1,
        column: pos.column + 1,
        span: (decl.start_byte(), decl.end_byte() - decl.start_byte()),
        signature,
    })
}

/// Append every leaf token under `node` (comments excluded) to `sig` as
/// `kind_id` + text, separated so two distinct token streams can never collide.
/// Counts the tokens so the caller can gate trivial bodies.
fn collect_body_tokens(node: Node, source: &[u8], sig: &mut Vec<u8>, count: &mut usize) {
    if node.is_error() || node.is_missing() {
        return;
    }
    if node.child_count() == 0 {
        if is_comment_kind(node.kind()) {
            return;
        }
        let Some(text) = source.get(node.start_byte()..node.end_byte()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        sig.extend_from_slice(&node.kind_id().to_le_bytes());
        sig.push(0);
        sig.extend_from_slice(text);
        sig.push(0);
        *count += 1;
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_body_tokens(child, source, sig, count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write(dir: &tempfile::TempDir, name: &str, content: &str) -> SourceFile {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        let language = match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => Language::Rust,
            Some("tsx") => Language::Tsx,
            Some("js") => Language::JavaScript,
            _ => Language::TypeScript,
        };
        SourceFile { path, language }
    }

    fn run(files: &[&SourceFile]) -> Vec<Diagnostic> {
        lint_files(files, &Config::default())
    }

    // The exact copy-paste from saurenya MR 1292.
    const CELL_TO_STRING: &str = "\
function cellToString(cell: unknown): string {
  if (typeof cell === \"string\") return cell;
  if (typeof cell === \"number\" || typeof cell === \"boolean\") return String(cell);
  return \"\";
}
";

    #[test]
    fn flags_duplicate_named_function_across_files() {
        // Regression (saurenya MR 1292): `cellToString` pasted verbatim into two
        // import readers. ~30 body tokens — under no-clones' 100-token window.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "direct-reader.ts", &format!("export const x = 1;\n{CELL_TO_STRING}"));
        let b = write(&dir, "hipra.ts", &format!("export const y = 2;\n{CELL_TO_STRING}"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "the pasted function must be flagged");
        assert_eq!(diags[0].rule_id, RULE_ID);
        assert!(diags[0].message.contains("`cellToString`"));
        // Canonical is the lexicographically-first path; hipra.ts reports it.
        assert!(diags[0].path.ends_with("hipra.ts"));
        assert!(diags[0].message.contains("direct-reader.ts"));
    }

    #[test]
    fn comment_inside_body_is_ignored() {
        // Whitespace and an extra comment inside one copy must not hide the dup.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let with_comment = "\
function cellToString(cell: unknown): string {
  // explain the primitive cases
  if (typeof cell === \"string\") return cell;
  if (typeof cell === \"number\" || typeof cell === \"boolean\")    return String(cell);
  return \"\";
}
";
        let b = write(&dir, "b.ts", with_comment);
        assert_eq!(run(&[&a, &b]).len(), 1, "comments and whitespace are not part of the body");
    }

    #[test]
    fn same_name_different_body_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let variant = "\
function cellToString(cell: unknown): string {
  if (typeof cell === \"string\") return cell.trim();
  if (typeof cell === \"number\" || typeof cell === \"boolean\") return String(cell);
  if (cell == null) return \"n/a\";
  return \"\";
}
";
        let b = write(&dir, "b.ts", variant);
        assert!(run(&[&a, &b]).is_empty(), "a different body is not a duplicate");
    }

    #[test]
    fn different_name_same_body_not_flagged() {
        // The shared name is the discriminator; identical bodies under different
        // names are no-clones' job, not this rule's.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let renamed = CELL_TO_STRING.replace("cellToString", "stringifyCell");
        let b = write(&dir, "b.ts", &renamed);
        assert!(run(&[&a, &b]).is_empty(), "different names are not duplicates");
    }

    #[test]
    fn trivial_function_below_threshold_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", "export function noop() {}\n");
        let b = write(&dir, "b.ts", "export function noop() {}\n");
        assert!(run(&[&a, &b]).is_empty(), "a body under min_body_tokens is ignored");
    }

    #[test]
    fn flags_duplicate_arrow_const() {
        let dir = tempfile::tempdir().unwrap();
        let arrow = CELL_TO_STRING
            .replace("function cellToString(cell: unknown): string {", "const cellToString = (cell: unknown): string => {")
            .replace("\n}\n", "\n};\n");
        let a = write(&dir, "a.ts", &arrow);
        let b = write(&dir, "b.ts", &arrow);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a duplicated arrow-const function is flagged");
        assert!(diags[0].message.contains("`cellToString`"));
    }

    #[test]
    fn formatting_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let reformatted = CELL_TO_STRING.replace("  ", "\t").replace(") return", ")\n    return");
        let b = write(&dir, "b.ts", &reformatted);
        assert_eq!(run(&[&a, &b]).len(), 1, "token-based match ignores formatting");
    }

    #[test]
    fn overload_signatures_not_flagged() {
        // The bodiless overload signatures are skipped (no `body` field); only the
        // implementation has a body, and the two implementations differ.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.ts",
            "function pick(x: number): number;\nfunction pick(x: string): string;\nfunction pick(x: unknown) { return x as number; }\n",
        );
        let b = write(
            &dir,
            "b.ts",
            "function pick(x: number): number;\nfunction pick(x: string): string;\nfunction pick(x: unknown) { return String(x); }\n",
        );
        assert!(run(&[&a, &b]).is_empty(), "overload signatures have no body and differ in impl");
    }

    #[test]
    fn bucket_of_three_yields_two_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", CELL_TO_STRING);
        let b = write(&dir, "b.ts", CELL_TO_STRING);
        let c = write(&dir, "c.ts", CELL_TO_STRING);
        let diags = run(&[&a, &b, &c]);
        assert_eq!(diags.len(), 2, "N files yield N-1 diagnostics");
        assert!(diags.iter().all(|d| d.message.contains("a.ts")));
        assert!(diags[0].path.ends_with("b.ts"));
        assert!(diags[1].path.ends_with("c.ts"));
    }

    #[test]
    fn relaxed_dir_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "samples/a.ts", CELL_TO_STRING);
        let b = write(&dir, "samples/b.ts", CELL_TO_STRING);
        assert!(run(&[&a, &b]).is_empty(), "fixture/sample dirs are exempt");
    }

    #[test]
    fn generated_content_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let gen_src = format!("// @generated\n{CELL_TO_STRING}");
        let a = write(&dir, "a.ts", &gen_src);
        let b = write(&dir, "b.ts", &gen_src);
        assert!(run(&[&a, &b]).is_empty(), "@generated files are exempt");
    }

    #[test]
    fn rust_not_flagged() {
        // Rust is out of scope for v1 — no-clones covers it.
        let dir = tempfile::tempdir().unwrap();
        let f = "pub fn cell_to_string(cell: &str) -> String {\n    if cell.is_empty() { return String::new(); }\n    cell.trim().to_string()\n}\n";
        let a = write(&dir, "a.rs", f);
        let b = write(&dir, "b.rs", f);
        assert!(run(&[&a, &b]).is_empty(), "Rust functions are not in scope");
    }

    #[test]
    fn single_file_is_noop() {
        let f = SourceFile {
            path: PathBuf::from("/tmp/only.ts"),
            language: Language::TypeScript,
        };
        assert!(run(&[&f]).is_empty());
    }
}
