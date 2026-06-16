//! Cross-file near-duplicate comment detection.
//!
//! Copy-pasted explanations are a smell: the copies drift out of sync until one
//! describes code it no longer matches. The signature of a copy-paste is a
//! *verbatim shared opening* — two comments that begin with the same run of
//! words and then diverge. This module flags those.
//!
//! - Extract comment blocks from every TS/JS/Rust file (consecutive `//` lines
//!   merge into one logical block; `/* */` blocks stand alone).
//! - Normalize to a lowercase word list and bucket by the first
//!   `shared_prefix_words` words.
//! - A bucket of two or more comments whose shared opening is distinctive
//!   enough (entropy gate) and long enough relative to the comment
//!   (`prefix_pct`) is reported.

use std::collections::HashSet;
use std::path::Path;

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use tree_sitter::Parser;

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Severity};
use crate::files::{Language, SourceFile};
use crate::parsing::parse_with_grammar;

pub const RULE_ID: &str = "no-duplicate-comments";

/// Minimum number of distinct non-stopword words the shared prefix must carry.
///
/// A shared prefix of N words is only a copy-paste signal when those words are
/// *distinctive*. Generic openers (`this function returns the value of the …`)
/// are shared by many unrelated comments; gating on prefix length alone would
/// bucket them all together and emit a flood of obvious non-duplicates. This is
/// the comment analogue of clone detection's distinct-trigram gate.
const MIN_DISTINCT_PREFIX_WORDS: usize = 4;

/// Words that carry no discriminating signal in a comment opener — English
/// function words plus doc-comment boilerplate (`function`, `returns`,
/// `value`, …). Only used by the entropy gate, never to alter the prefix
/// itself, so two comments must still share the verbatim opener to collide.
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "of", "to", "for", "in", "on", "and", "or", "so", "as", "is", "are", "be",
    "was", "were", "this", "that", "these", "those", "it", "its", "with", "from", "by", "at",
    "into", "but", "if", "then", "than", "when", "which", "we", "you", "i", "our", "your", "their",
    "they", "not", "no", "can", "will", "should", "must", "may", "do", "does", "has", "have", "had",
    "been", "being", "there", "here", "about", "per", "via", "up", "out", "all", "any", "each",
    "some", "such", "only", "also", "more", "most", "other", "one", "new", "function", "functions",
    "returns", "return", "value", "values", "creates", "create", "instance", "helper", "method",
    "methods", "gets", "get", "sets", "set", "true", "false", "given", "used", "use", "uses",
    "using", "call", "called", "calls", "check", "checks", "handle", "handles", "handler", "note",
    "param", "params", "parameter", "argument", "arg", "args",
];

fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word)
}

fn is_target_language(lang: Language) -> bool {
    matches!(
        lang,
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Rust
    )
}

/// Translation/locale trees legitimately repeat the same explanatory comment
/// across every locale sibling — that is the format working as intended, not a
/// copy-paste smell.
fn is_locale_path(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str().to_str().is_some_and(|s| {
            matches!(s, "locale" | "locales" | "i18n" | "translations" | "translation")
        })
    })
}

fn is_comment_kind(kind: &str) -> bool {
    matches!(kind, "comment" | "line_comment" | "block_comment")
}

/// License banners and tooling directives are duplicated by design — never a
/// copy-paste smell — and are matched by content, not file position, so a
/// banner below `#![attr]` / `'use client'` / a shebang is still caught and a
/// Rust `//!` module doc is not.
const EXCLUDE_MARKERS: &[&str] = &[
    "spdx-license-identifier",
    "copyright",
    "licensed under",
    "eslint-disable",
    "@ts-",
    "biome-ignore",
    "prettier-ignore",
    "stylelint-disable",
    "tslint:",
    "comply-ignore",
];

fn is_excluded_comment(lower: &str) -> bool {
    EXCLUDE_MARKERS.iter().any(|m| lower.contains(m))
}

/// True when one `//` line is itself a directive/banner. Used to split a run of
/// consecutive line comments so a directive (e.g. `// comply-ignore: …`) never
/// merges into the prose docblock beneath it — otherwise the merged block would
/// inherit the marker and the real comment would be wrongly excluded.
fn line_is_marker(stripped_line: &str) -> bool {
    let lower = stripped_line.to_lowercase();
    EXCLUDE_MARKERS.iter().any(|m| lower.contains(m))
}

/// A comment eligible to be compared against others.
struct CommentEntry {
    file_idx: usize,
    line: usize,
    column: usize,
    span: (usize, usize),
    words: Vec<String>,
    /// The first `shared_prefix_words` words, joined — the bucket key.
    prefix_key: String,
}

/// A logical comment block: either one `/* */` node or a run of consecutive
/// `//` lines that share a start column.
struct CommentGroup {
    line: usize,
    column: usize,
    start_byte: usize,
    end_byte: usize,
    stripped: String,
}

struct RawComment {
    start_byte: usize,
    end_byte: usize,
    row: usize,
    col: usize,
    is_line: bool,
}

#[must_use]
pub fn lint_files(files: &[&SourceFile], config: &Config) -> Vec<Diagnostic> {
    let files: Vec<&SourceFile> = files
        .iter()
        .copied()
        .filter(|f| is_target_language(f.language))
        .filter(|f| {
            !crate::rules::file_ctx::scan_path(&f.path).is_relaxed_dir
                && !crate::rules::file_ctx::is_generated_path(&f.path)
                && !is_locale_path(&f.path)
        })
        .collect();

    if files.len() < 2 {
        return vec![];
    }

    // Cross-language rule: its knobs live in a single non-per-language
    // `[rules.<id>]` block, so the `Language` passed to the config lookup is
    // immaterial — any value reads the same number.
    let min_words = config.threshold(RULE_ID, "min_words", Language::TypeScript);
    let prefix_words = config.threshold(RULE_ID, "shared_prefix_words", Language::TypeScript);
    let prefix_pct = config.float(RULE_ID, "prefix_pct", Language::TypeScript);

    let entries: Vec<CommentEntry> = files
        .par_iter()
        .enumerate()
        .map_init(Parser::new, |parser, (idx, file)| {
            extract_entries(parser, file, idx, min_words, prefix_words)
        })
        .flatten()
        .collect();

    let mut buckets: FxHashMap<&str, Vec<usize>> = FxHashMap::default();
    for (i, entry) in entries.iter().enumerate() {
        buckets.entry(entry.prefix_key.as_str()).or_default().push(i);
    }

    let mut diags = Vec::new();
    for members in buckets.values() {
        if members.len() < 2 {
            continue;
        }
        // Sort by (path, line) so both the partner choice and the output order
        // are deterministic. Each member after the first reports once, against
        // the earlier member it shares the longest opening with — so a bucket
        // of N yields at most N-1 diagnostics, each pointing at a genuinely
        // similar sibling rather than an arbitrary bucket representative.
        let mut ordered = members.clone();
        ordered.sort_by(|&a, &b| {
            let (ea, eb) = (&entries[a], &entries[b]);
            files[ea.file_idx]
                .path
                .cmp(&files[eb.file_idx].path)
                .then(ea.line.cmp(&eb.line))
        });
        for (pos, &m) in ordered.iter().enumerate().skip(1) {
            let entry = &entries[m];
            // Closest earlier sibling: longest shared opening. Ties resolve to
            // the earliest, since we only replace on a strictly longer match.
            // Every member shares the bucket's prefix, so `shared >= prefix_words`.
            let mut shared = 0;
            let mut partner_idx = ordered[0];
            for &p in &ordered[..pos] {
                let lcp = common_prefix_len(&entries[p].words, &entry.words);
                if lcp > shared {
                    shared = lcp;
                    partner_idx = p;
                }
            }
            let partner = &entries[partner_idx];
            let shorter = partner.words.len().min(entry.words.len());
            if (shared as f64) < prefix_pct * (shorter as f64) {
                continue;
            }
            // Tailor the remediation to reach: a copy in the same file is best
            // collapsed to one comment the others reference, while the same
            // rationale spread across files belongs in a doc the comments cite.
            let remediation = if entry.file_idx == partner.file_idx {
                "Keep one comment and point the rest at it."
            } else {
                "Lift the shared rationale into an ADR or canonical doc the comments cite."
            };
            diags.push(Diagnostic {
                path: std::sync::Arc::from(files[entry.file_idx].path.as_path()),
                line: entry.line,
                column: entry.column,
                rule_id: RULE_ID.into(),
                message: format!(
                    "Near-duplicate comment: its {shared}-word opening also appears in `{}` at \
                     line {}. Copy-pasted explanations drift out of sync until one describes code \
                     it no longer matches. {remediation}",
                    files[partner.file_idx].path.display(),
                    partner.line,
                ),
                severity: Severity::Warning,
                span: Some(entry.span),
            });
        }
    }

    diags.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    diags
}

fn common_prefix_len(a: &[String], b: &[String]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

fn extract_entries(
    parser: &mut Parser,
    file: &SourceFile,
    file_idx: usize,
    min_words: usize,
    prefix_words: usize,
) -> Vec<CommentEntry> {
    let Ok(source) = std::fs::read_to_string(&file.path) else {
        return Vec::new();
    };
    if crate::rules::file_ctx::is_generated_content(&source) {
        return Vec::new();
    }
    let Some(tree) = parse_with_grammar(parser, file.language, source.as_bytes()) else {
        return Vec::new();
    };

    let raws = collect_raw_comments(&tree, source.as_bytes());
    let groups = merge_groups(&raws, &source);

    // `prefix` slicing requires at least `prefix_words`; never index past the
    // word count even if a project misconfigures `min_words` below it.
    let need = min_words.max(prefix_words);

    let mut entries = Vec::new();
    for group in groups {
        if is_excluded_comment(&group.stripped.to_lowercase()) {
            continue;
        }
        let words = normalize_words(&group.stripped);
        if words.len() < need {
            continue;
        }
        let prefix = &words[..prefix_words];
        if !prefix_passes_entropy(prefix) {
            continue;
        }
        let prefix_key = prefix.join("\u{1}");
        entries.push(CommentEntry {
            file_idx,
            line: group.line,
            column: group.column,
            span: (group.start_byte, group.end_byte - group.start_byte),
            words,
            prefix_key,
        });
    }
    entries
}

fn collect_raw_comments(tree: &tree_sitter::Tree, source: &[u8]) -> Vec<RawComment> {
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if is_comment_kind(node.kind()) {
            let start = node.start_byte();
            let end = node.end_byte();
            let is_line = source[start..end].starts_with(b"//");
            out.push(RawComment {
                start_byte: start,
                end_byte: end,
                row: node.start_position().row,
                col: node.start_position().column,
                is_line,
            });
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                out.sort_by_key(|c| c.start_byte);
                return out;
            }
        }
    }
}

fn merge_groups(raws: &[RawComment], source: &str) -> Vec<CommentGroup> {
    let mut groups = Vec::new();
    let mut i = 0;
    while i < raws.len() {
        let c = &raws[i];
        if c.is_line {
            let col = c.col;
            let first_text = strip_line_marker(&source[c.start_byte..c.end_byte]);
            let group_is_marker = line_is_marker(first_text);
            let mut last_row = c.row;
            let mut end_byte = c.end_byte;
            let mut texts = vec![first_text.to_string()];
            let mut j = i + 1;
            while let Some(n) = raws.get(j) {
                if !(n.is_line && n.col == col && n.row == last_row + 1) {
                    break;
                }
                let n_text = strip_line_marker(&source[n.start_byte..n.end_byte]);
                // A directive line and a prose line never share a block.
                if line_is_marker(n_text) != group_is_marker {
                    break;
                }
                texts.push(n_text.to_string());
                last_row = n.row;
                end_byte = n.end_byte;
                j += 1;
            }
            groups.push(CommentGroup {
                line: c.row + 1,
                column: c.col + 1,
                start_byte: c.start_byte,
                end_byte,
                stripped: texts.join(" "),
            });
            i = j;
        } else {
            groups.push(CommentGroup {
                line: c.row + 1,
                column: c.col + 1,
                start_byte: c.start_byte,
                end_byte: c.end_byte,
                stripped: strip_block(&source[c.start_byte..c.end_byte]),
            });
            i += 1;
        }
    }
    groups
}

fn strip_line_marker(raw: &str) -> &str {
    let trimmed = raw.trim_start();
    let inner = trimmed
        .strip_prefix("///")
        .or_else(|| trimmed.strip_prefix("//!"))
        .or_else(|| trimmed.strip_prefix("//"))
        .unwrap_or(trimmed);
    inner.trim()
}

fn strip_block(raw: &str) -> String {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix("/**")
        .or_else(|| trimmed.strip_prefix("/*"))
        .unwrap_or(trimmed);
    let inner = inner.strip_suffix("*/").unwrap_or(inner);
    inner
        .lines()
        .map(|l| l.trim().trim_start_matches('*').trim())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_words(stripped: &str) -> Vec<String> {
    stripped
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn prefix_passes_entropy(prefix: &[String]) -> bool {
    let mut distinct: HashSet<&str> = HashSet::new();
    for word in prefix {
        if !is_stopword(word) {
            distinct.insert(word.as_str());
        }
    }
    distinct.len() >= MIN_DISTINCT_PREFIX_WORDS
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

    const C1: &str = "\
// Defaults derived from the canonical schema so a change to `statut` / `sort`
// / a new filter default stays in sync; `pageSize` overridden to the
// admin-list bound (one fixed page covers the fixed réseau set).
export const adminList = 1;
";

    const C2: &str = "\
// Defaults derived from the canonical schema so a change to `sort` / a new
// filter default stays in sync; `laboratoryId` scopes the section and
// `pageSize` is the section's first-page bound.
export const labSection = 2;
";

    #[test]
    fn flags_near_duplicate_doc_comments() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "admin.ts", C1);
        let b = write(&dir, "lab.ts", C2);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "expected one near-duplicate diagnostic");
        assert_eq!(diags[0].rule_id, RULE_ID);
        assert!(diags[0].message.contains("Near-duplicate comment"));
        // Canonical is the lexicographically-first path (admin.ts); lab.ts reports.
        assert!(diags[0].path.ends_with("lab.ts"));
        assert!(diags[0].message.contains("admin.ts"));
        // Cross-file remediation points at a shared doc.
        assert!(diags[0].message.contains("ADR or canonical doc"));
    }

    #[test]
    fn flags_intra_file_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let content = format!("{C1}\n{C2}");
        let a = write(&dir, "a.ts", &content);
        let b = write(&dir, "filler.ts", "export const x = 1;\n");
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "two similar comments in one file are flagged");
        assert!(diags[0].path.ends_with("a.ts"));
        // Intra-file remediation collapses to a single comment.
        assert!(diags[0].message.contains("Keep one comment"));
    }

    #[test]
    fn bucket_of_three_yields_two_diagnostics() {
        // N members → N-1 diagnostics, each pointing at the earliest sibling.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", C1);
        let b = write(&dir, "b.ts", C1);
        let c = write(&dir, "c.ts", C1);
        let diags = run(&[&a, &b, &c]);
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.message.contains("a.ts")));
        assert!(diags[0].path.ends_with("b.ts"));
        assert!(diags[1].path.ends_with("c.ts"));
    }

    #[test]
    fn span_bytes_are_correct_after_multibyte() {
        // Multibyte content before the comment pushes its byte offset past its
        // char offset; the reported span must still slice to the comment text.
        let dir = tempfile::tempdir().unwrap();
        let line = "const \u{2615} = 1; // Builds the canonical pagination defaults derived from the shared schema so every list stays consistent.\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1);
        let (offset, len) = diags[0].span.unwrap();
        let slice = &line[offset..offset + len];
        assert!(slice.starts_with("//"));
        assert!(slice.contains("Builds the canonical"));
    }

    #[test]
    fn ignores_generic_boilerplate_opener() {
        // Both share the first eight words ("this function returns the value of
        // the configured") but those are almost all stopwords — the entropy
        // gate must keep this out, or every doc comment collides.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.ts",
            "// This function returns the value of the configured setting loaded from disk and cached for reuse later.\nexport const a = 1;\n",
        );
        let b = write(
            &dir,
            "b.ts",
            "// This function returns the value of the configured option pulled from cache and validated before returning.\nexport const b = 2;\n",
        );
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn ignores_short_comments() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.ts", "// fetch the user record\nexport const a = 1;\n");
        let b = write(&dir, "b.ts", "// fetch the user record\nexport const b = 2;\n");
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn ignores_license_headers() {
        let dir = tempfile::tempdir().unwrap();
        let banner = "// Copyright (c) 2026 Acme Corp. All rights reserved. This source file is part of the project and licensed for internal use only.\n";
        let a = write(&dir, "a.ts", &format!("{banner}export const a = 1;\n"));
        let b = write(&dir, "b.ts", &format!("{banner}export const b = 2;\n"));
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn flags_duplicate_block_comments() {
        let dir = tempfile::tempdir().unwrap();
        let block = "/**\n * Builds the canonical pagination defaults derived from the shared schema so every list stays consistent.\n */\n";
        let a = write(&dir, "a.ts", &format!("{block}export const a = 1;\n"));
        let b = write(&dir, "b.ts", &format!("{block}export const b = 2;\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RULE_ID);
    }

    #[test]
    fn flags_rust_doc_comments() {
        let dir = tempfile::tempdir().unwrap();
        let doc = "/// Builds the canonical pagination defaults derived from the shared schema so every list stays consistent everywhere.\npub fn f() {}\n";
        let a = write(&dir, "a.rs", doc);
        let b = write(&dir, "b.rs", doc);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn directive_line_does_not_swallow_following_docblock() {
        // Regression (saurenya MR 1275): a `// comply-ignore` / `// eslint-disable`
        // line directly above a prose docblock at the same indent must not merge
        // into it and drag the whole block into the directive exclusion.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.ts",
            &format!("// comply-ignore: some-rule — justification text goes here.\n{C1}"),
        );
        let b = write(
            &dir,
            "b.ts",
            &format!("// eslint-disable-next-line no-shadow\n{C2}"),
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "docblock below a directive line is still compared");
        assert!(diags[0].message.contains("Near-duplicate comment"));
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
