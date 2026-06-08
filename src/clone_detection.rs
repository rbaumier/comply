//! Native token-based clone detection (Rabin-Karp).
//!
//! - Parse each file with tree-sitter, extract leaf tokens
//! - Hash sliding windows of MIN_TOKENS tokens
//! - Cross-file collisions with token-by-token verification = clones

use rustc_hash::FxHashMap;

use rayon::prelude::*;
use tree_sitter::Parser;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::{Language, SourceFile};
use crate::parsing;

pub const RULE_ID: &str = "no-clones";
const MIN_TOKENS: usize = 100;
/// Polynomial multiplier for the sliding-window Rabin-Karp hash.
const WINDOW_HASH_MULT: u64 = 6_364_136_223_846_793_005;
const BUCKET_SATURATED: usize = 64;
/// Minimum number of distinct token-text trigrams required in a match window.
///
/// Filters out low-entropy boilerplate (e.g. registration tables of
/// `(Lang, Backend::TreeSitter(Box::new(foo::Check)))`) where 100+ tokens
/// can match exactly across files but consist mostly of repeated 3-token
/// subsequences. Genuine duplicated logic — each statement carrying fresh
/// identifiers — yields many more distinct trigrams than this threshold.
const MIN_DISTINCT_TRIGRAMS: usize = 145;
/// Maximum non-covered token gap that marks a file pair as "symmetric siblings"
/// rather than accidental duplicates.  When two files are identical except for
/// one small load-bearing block (e.g. deactivate vs. reactivate handlers), the
/// tokens in that block are not part of any matching window.  If the reporter
/// file has fewer than this many uncovered tokens, the pair is suppressed.
/// Must be strictly less than the gap produced by the `merge_refuses_non_adjacent_canonical`
/// test (52 tokens with the current test parameters).
const SYMMETRIC_SIBLING_GAP_THRESHOLD: usize = MIN_TOKENS / 2; // 50

struct Token {
    kind_id: u16,
    start_byte: usize,
    end_byte: usize,
    line: usize,
    hash: u64,
}

struct FileTokens {
    source: Vec<u8>,
    tokens: Vec<Token>,
}

struct Occurrence {
    file_idx: usize,
    start_token: usize,
    start_line: usize,
}

// (reporter_fi, reporter_tok_start, reporter_line, canonical_fi, canonical_tok_start, canonical_line)
type RawClone = (usize, usize, usize, usize, usize, usize);

#[must_use]
pub fn lint_files(files: &[&SourceFile]) -> Vec<Diagnostic> {
    if files.len() < 2 {
        return vec![];
    }

    let file_data: Vec<Option<FileTokens>> = files
        .par_iter()
        .map_init(Parser::new, |parser, file| tokenize_file(parser, file))
        .collect();

    let mut raw = find_raw_clones(&file_data);
    merge_and_emit(&mut raw, &file_data, files)
}

fn find_raw_clones(file_data: &[Option<FileTokens>]) -> Vec<RawClone> {
    // Weight of the token leaving the window when rolling one step right, i.e.
    // `WINDOW_HASH_MULT^(MIN_TOKENS - 1)`.
    let k_pow: u64 = WINDOW_HASH_MULT.wrapping_pow((MIN_TOKENS - 1) as u32);

    let mut index: FxHashMap<u64, Vec<Occurrence>> = FxHashMap::default();
    let mut raw: Vec<RawClone> = Vec::new();

    for (fi, ft) in file_data.iter().enumerate() {
        let Some(ft) = ft else { continue };
        if ft.tokens.len() < MIN_TOKENS {
            continue;
        }
        // Rabin-Karp rolling hash: hash the first window in full, then derive
        // each subsequent window in O(1) by dropping the outgoing token and
        // folding in the incoming one. Yields the same value as `window_hash`
        // applied to each window, at ~MIN_TOKENS× less work.
        let n_windows = ft.tokens.len() - MIN_TOKENS + 1;
        let mut wh = window_hash(&ft.tokens[0..MIN_TOKENS]);
        for start in 0..n_windows {
            if start > 0 {
                let outgoing = ft.tokens[start - 1].hash;
                let incoming = ft.tokens[start + MIN_TOKENS - 1].hash;
                wh = wh
                    .wrapping_sub(outgoing.wrapping_mul(k_pow))
                    .wrapping_mul(WINDOW_HASH_MULT)
                    .wrapping_add(incoming);
            }

            let bucket = index.entry(wh).or_default();

            if bucket.len() >= BUCKET_SATURATED {
                continue;
            }

            let mut matched = false;
            for occ in bucket.iter() {
                if occ.file_idx != fi
                    && let Some(ref canon_ft) = file_data[occ.file_idx]
                    && verify_tokens(ft, start, canon_ft, occ.start_token)
                {
                    raw.push((
                        fi,
                        start,
                        ft.tokens[start].line,
                        occ.file_idx,
                        occ.start_token,
                        occ.start_line,
                    ));
                    matched = true;
                    break;
                }
            }

            // Matched windows skip insertion — future duplicates match the canonical.
            if !matched {
                bucket.push(Occurrence {
                    file_idx: fi,
                    start_token: start,
                    start_line: ft.tokens[start].line,
                });
            }
        }
    }

    raw
}

fn verify_tokens(a: &FileTokens, a_start: usize, b: &FileTokens, b_start: usize) -> bool {
    for i in 0..MIN_TOKENS {
        let ta = &a.tokens[a_start + i];
        let tb = &b.tokens[b_start + i];
        if ta.kind_id != tb.kind_id {
            return false;
        }
        if a.source[ta.start_byte..ta.end_byte] != b.source[tb.start_byte..tb.end_byte] {
            return false;
        }
    }
    true
}

/// Diversity gate. Counts distinct token-text trigrams across the
/// `[first_tok, last_tok]` range of `ft` (inclusive of the trailing
/// `MIN_TOKENS`-wide window starting at `last_tok`). Trigrams penalise
/// repeated subsequences — e.g. a registration table with four
/// `(Language::X, Backend::TreeSitter(Box::new(typescript::Check)))`
/// rows yields very few distinct trigrams, while genuine duplicated
/// logic (each statement carrying fresh identifiers) yields many.
/// The clone is rejected when the merged span has fewer than
/// `MIN_DISTINCT_TRIGRAMS` distinct trigrams.
fn has_enough_distinct_texts(ft: &FileTokens, first_tok: usize, last_window_tok: usize) -> bool {
    use std::collections::HashSet;
    let last_tok = (last_window_tok + MIN_TOKENS - 1).min(ft.tokens.len() - 1);
    if last_tok < first_tok + 2 {
        return false;
    }
    let mut seen: HashSet<(&[u8], &[u8], &[u8])> = HashSet::new();
    for i in first_tok..=last_tok - 2 {
        let a = &ft.tokens[i];
        let b = &ft.tokens[i + 1];
        let c = &ft.tokens[i + 2];
        let ta = &ft.source[a.start_byte..a.end_byte];
        let tb = &ft.source[b.start_byte..b.end_byte];
        let tc = &ft.source[c.start_byte..c.end_byte];
        seen.insert((ta, tb, tc));
        if seen.len() >= MIN_DISTINCT_TRIGRAMS {
            return true;
        }
    }
    seen.len() >= MIN_DISTINCT_TRIGRAMS
}

fn merge_and_emit(
    raw: &mut Vec<RawClone>,
    file_data: &[Option<FileTokens>],
    files: &[&SourceFile],
) -> Vec<Diagnostic> {
    if raw.is_empty() {
        return vec![];
    }
    raw.sort_unstable();
    raw.dedup();

    struct Span {
        rfi: usize,
        rstart: usize,
        last_rstart: usize,
        rline: usize,
        cfi: usize,
        cstart: usize,
        cline: usize,
    }

    // Phase 1 — merge adjacent windows into spans, apply diversity gate.
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let (rfi, rstart, rline, cfi, cstart, cline) = raw[i];
        let mut last_rstart = rstart;
        let mut last_cstart = cstart;
        let mut j = i + 1;
        while j < raw.len() {
            let (nrfi, nrstart, _, ncfi, ncstart, _) = raw[j];
            if nrfi == rfi
                && ncfi == cfi
                && nrstart == last_rstart + 1
                && ncstart == last_cstart + 1
            {
                last_rstart = nrstart;
                last_cstart = ncstart;
                j += 1;
                continue;
            }
            break;
        }
        // Identifier-diversity gate — filters low-entropy boilerplate clones.
        let keep = file_data[rfi]
            .as_ref()
            .is_some_and(|ft| has_enough_distinct_texts(ft, rstart, last_rstart));
        if keep {
            spans.push(Span { rfi, rstart, last_rstart, rline, cfi, cstart, cline });
        }
        i = j;
    }

    if spans.is_empty() {
        return vec![];
    }

    // Phase 2 — suppress symmetric sibling pairs.
    // Two suppression criteria, either is sufficient:
    //
    // A) Small-gap: the tokens NOT covered by any matching window in the
    //    reporter file are few (> 0, ≤ SYMMETRIC_SIBLING_GAP_THRESHOLD).  A
    //    small non-zero gap signals one load-bearing difference surrounded by
    //    identical structure (e.g. the `.set()` argument between two handlers).
    //
    // B) Named complement: the two files live in the same directory and their
    //    stems form a known domain complement pair (deactivate/reactivate,
    //    enable/disable, …).  These implement symmetric operations that
    //    intentionally share query structure — the shared block is not
    //    accidental copy-paste.
    let mut suppressed = std::collections::HashSet::<usize>::new();
    {
        let mut by_pair: FxHashMap<(usize, usize), Vec<usize>> = FxHashMap::default();
        for (idx, s) in spans.iter().enumerate() {
            by_pair.entry((s.rfi, s.cfi)).or_default().push(idx);
        }
        for ((rfi, cfi), idxs) in &by_pair {
            let total = file_data[*rfi].as_ref().map_or(0, |ft| ft.tokens.len());
            let covered: usize = idxs
                .iter()
                .map(|&i| spans[i].last_rstart - spans[i].rstart + MIN_TOKENS)
                .sum();
            let gap = total.saturating_sub(covered);
            let small_gap = gap > 0 && gap <= SYMMETRIC_SIBLING_GAP_THRESHOLD;
            let name_siblings = gap > 0
                && are_symmetric_name_pair(&files[*rfi].path, &files[*cfi].path);
            if small_gap || name_siblings {
                suppressed.extend(idxs);
            }
        }
    }

    // Phase 3 — emit diagnostics for non-suppressed spans.
    spans
        .iter()
        .enumerate()
        .filter(|(idx, _)| !suppressed.contains(idx))
        .map(|(_, s)| {
            let lines = clone_line_span(file_data, s.rfi, s.rstart, s.last_rstart);
            Diagnostic {
                path: std::sync::Arc::from(files[s.rfi].path.as_path()),
                line: s.rline,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "Duplicated block ({lines} lines) — also in `{}` at line {}.",
                    files[s.cfi].path.display(),
                    s.cline,
                ),
                severity: Severity::Warning,
                span: None,
            }
        })
        .collect()
}

fn clone_line_span(
    file_data: &[Option<FileTokens>],
    fi: usize,
    first_tok: usize,
    last_window_tok: usize,
) -> usize {
    let Some(ref ft) = file_data[fi] else {
        debug_assert!(false, "clone_line_span called with None file_data[{fi}]");
        return MIN_TOKENS;
    };
    let first_line = ft.tokens[first_tok].line;
    let last_tok_idx = last_window_tok + MIN_TOKENS - 1;
    let last_line = if last_tok_idx < ft.tokens.len() {
        ft.tokens[last_tok_idx].line
    } else {
        ft.tokens.last().map_or(first_line, |t| t.line)
    };
    last_line.saturating_sub(first_line) + 1
}

// --- Symmetric-sibling detection ---

/// Returns true if `word` appears in `text` surrounded by non-alphanumeric
/// characters (or string boundaries), using byte-level comparison.
fn contains_word(text: &str, word: &str) -> bool {
    let tb = text.as_bytes();
    let wb = word.as_bytes();
    let wlen = wb.len();
    if wlen > tb.len() {
        return false;
    }
    for i in 0..=(tb.len() - wlen) {
        if &tb[i..i + wlen] == wb {
            let before_ok = i == 0 || !tb[i - 1].is_ascii_alphanumeric();
            let after_ok = i + wlen == tb.len() || !tb[i + wlen].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

/// Returns true when two paths are in the same directory and their stems form
/// a known complement pair (e.g. `deactivate-product` / `reactivate-product`).
/// Such pairs implement symmetric domain operations and intentionally share
/// query structure — their shared block must not be flagged as a clone.
fn are_symmetric_name_pair(a: &std::path::Path, b: &std::path::Path) -> bool {
    if a.parent() != b.parent() {
        return false;
    }
    let stem_a = a
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let stem_b = b
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    const PAIRS: &[(&str, &str)] = &[
        ("deactivate", "reactivate"),
        ("enable", "disable"),
        ("publish", "unpublish"),
        ("lock", "unlock"),
        ("start", "stop"),
        ("open", "close"),
        ("add", "remove"),
        ("show", "hide"),
        ("mount", "unmount"),
        ("create", "delete"),
    ];
    for &(x, y) in PAIRS {
        if (contains_word(&stem_a, x) && contains_word(&stem_b, y))
            || (contains_word(&stem_a, y) && contains_word(&stem_b, x))
        {
            return true;
        }
    }
    false
}

// --- Tokenization ---

fn tokenize_file(parser: &mut Parser, file: &SourceFile) -> Option<FileTokens> {
    let grammar_tag = grammar_family(file.language)?;
    let source_str = std::fs::read_to_string(&file.path).ok()?;
    let source = source_str.into_bytes();
    let tree = parsing::parse_with_grammar(parser, file.language, &source)?;
    let mut tokens = Vec::new();
    let mut cursor = tree.walk();

    loop {
        let node = cursor.node();

        if node.is_error() || node.is_missing() {
            if !advance_to_next_sibling(&mut cursor) {
                break;
            }
            continue;
        }

        if node.child_count() == 0 && !is_comment_kind(node.kind()) {
            let kind_id = node.kind_id();
            let start_byte = node.start_byte();
            let end_byte = node.end_byte();
            let line = node.start_position().row + 1;
            let hash = token_hash(grammar_tag, kind_id, &source[start_byte..end_byte]);
            tokens.push(Token {
                kind_id,
                start_byte,
                end_byte,
                line,
                hash,
            });
        }

        if cursor.goto_first_child() {
            continue;
        }

        if !advance_to_next_sibling(&mut cursor) {
            break;
        }
    }

    Some(FileTokens { source, tokens })
}

fn advance_to_next_sibling(cursor: &mut tree_sitter::TreeCursor) -> bool {
    loop {
        if cursor.goto_next_sibling() {
            return true;
        }
        if !cursor.goto_parent() {
            return false;
        }
    }
}

fn is_comment_kind(kind: &str) -> bool {
    matches!(kind, "comment" | "line_comment" | "block_comment")
}

fn grammar_family(lang: Language) -> Option<u8> {
    match lang {
        Language::TypeScript | Language::JavaScript => Some(0),
        Language::Tsx => Some(1),
        Language::Rust => Some(2),
        Language::Css => Some(3),
        Language::Yaml => Some(4),
        Language::Dockerfile => Some(5),
        Language::Vue | Language::Toml | Language::Json | Language::Sql | Language::GraphQl | Language::Svelte => None,
    }
}

// --- Hashing ---

fn token_hash(grammar_tag: u8, kind_id: u16, text: &[u8]) -> u64 {
    let mut h: u64 = 0;
    h = hash_step(h, u64::from(grammar_tag));
    h = hash_step(h, u64::from(kind_id));
    h = hash_step(h, 0xFF);
    for &b in text {
        h = hash_step(h, u64::from(b));
    }
    h
}

fn window_hash(tokens: &[Token]) -> u64 {
    let mut h: u64 = 0;
    for t in tokens {
        h = h.wrapping_mul(WINDOW_HASH_MULT).wrapping_add(t.hash);
    }
    h
}

fn hash_step(h: u64, val: u64) -> u64 {
    (h.rotate_left(5) ^ val).wrapping_mul(0x517c_c1b7_2722_0a95)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_file(path: &str, lang: Language) -> SourceFile {
        SourceFile {
            path: PathBuf::from(path),
            language: lang,
        }
    }

    fn write_pair(dir: &tempfile::TempDir, ext: &str, content: &str) -> (SourceFile, SourceFile) {
        let pa = dir.path().join(format!("a.{ext}"));
        let pb = dir.path().join(format!("b.{ext}"));
        std::fs::write(&pa, content).unwrap();
        std::fs::write(&pb, content).unwrap();
        let lang = match ext {
            "ts" => Language::TypeScript,
            "rs" => Language::Rust,
            "js" => Language::JavaScript,
            "tsx" => Language::Tsx,
            _ => Language::TypeScript,
        };
        (
            SourceFile {
                path: pa,
                language: lang,
            },
            SourceFile {
                path: pb,
                language: lang,
            },
        )
    }

    fn large_ts_block(n: usize) -> String {
        (1..=n)
            .map(|i| format!("const value_{i} = computeExpensive({i}, \"param_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn large_rust_block(n: usize) -> String {
        (1..=n)
            .map(|i| format!("let value_{i} = compute_expensive({i}, \"param_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn no_clones_with_single_file() {
        let f = make_file("/tmp/a.ts", Language::TypeScript);
        assert!(lint_files(&[&f]).is_empty());
    }

    #[test]
    fn detects_clone_between_ts_files() {
        let dir = tempfile::tempdir().unwrap();
        let block = large_ts_block(20);
        let (fa, fb) = write_pair(&dir, "ts", &block);
        let diags = lint_files(&[&fa, &fb]);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-clones");
        assert!(diags[0].message.contains("lines"));
    }

    #[test]
    fn no_false_positive_on_short_overlap() {
        let dir = tempfile::tempdir().unwrap();
        let block = large_ts_block(5);
        let (fa, fb) = write_pair(&dir, "ts", &block);
        assert!(lint_files(&[&fa, &fb]).is_empty());
    }

    #[test]
    fn no_match_on_renamed_identifiers() {
        let dir = tempfile::tempdir().unwrap();
        let block_a: String = (1..=20)
            .map(|i| format!("const alpha_{i} = computeExpensive({i}, \"param_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n");
        let block_b: String = (1..=20)
            .map(|i| format!("const beta_{i} = computeExpensive({i}, \"param_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n");
        let pa = dir.path().join("a.ts");
        let pb = dir.path().join("b.ts");
        std::fs::write(&pa, &block_a).unwrap();
        std::fs::write(&pb, &block_b).unwrap();
        let fa = SourceFile {
            path: pa,
            language: Language::TypeScript,
        };
        let fb = SourceFile {
            path: pb,
            language: Language::TypeScript,
        };
        assert!(lint_files(&[&fa, &fb]).is_empty());
    }

    #[test]
    fn works_on_rust_files() {
        let dir = tempfile::tempdir().unwrap();
        let block = format!("fn main() {{\n{}\n}}", large_rust_block(20));
        let (fa, fb) = write_pair(&dir, "rs", &block);
        let diags = lint_files(&[&fa, &fb]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn js_and_ts_clones_detected() {
        let dir = tempfile::tempdir().unwrap();
        let block = large_ts_block(20);
        let pa = dir.path().join("a.js");
        let pb = dir.path().join("b.ts");
        std::fs::write(&pa, &block).unwrap();
        std::fs::write(&pb, &block).unwrap();
        let fa = SourceFile {
            path: pa,
            language: Language::JavaScript,
        };
        let fb = SourceFile {
            path: pb,
            language: Language::TypeScript,
        };
        let diags = lint_files(&[&fa, &fb]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn formatting_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let compact: String = (1..=20)
            .map(|i| format!("const value_{i}=computeExpensive({i},\"param_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n");
        let spaced: String = (1..=20)
            .map(|i| format!("  const  value_{i} = computeExpensive( {i} , \"param_{i}\" ) ;"))
            .collect::<Vec<_>>()
            .join("\n");
        let pa = dir.path().join("a.ts");
        let pb = dir.path().join("b.ts");
        std::fs::write(&pa, &compact).unwrap();
        std::fs::write(&pb, &spaced).unwrap();
        let fa = SourceFile {
            path: pa,
            language: Language::TypeScript,
        };
        let fb = SourceFile {
            path: pb,
            language: Language::TypeScript,
        };
        let diags = lint_files(&[&fa, &fb]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_comments() {
        let dir = tempfile::tempdir().unwrap();
        let with_comments: String = (1..=20)
            .map(|i| {
                format!("// comment {i}\nconst value_{i} = computeExpensive({i}, \"param_{i}\");")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let without: String = (1..=20)
            .map(|i| format!("const value_{i} = computeExpensive({i}, \"param_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n");
        let pa = dir.path().join("a.ts");
        let pb = dir.path().join("b.ts");
        std::fs::write(&pa, &with_comments).unwrap();
        std::fs::write(&pb, &without).unwrap();
        let fa = SourceFile {
            path: pa,
            language: Language::TypeScript,
        };
        let fb = SourceFile {
            path: pb,
            language: Language::TypeScript,
        };
        let diags = lint_files(&[&fa, &fb]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn hash_collision_with_first_mismatch() {
        // File 0 and 2 share bytes (varied per-token to clear the diversity
        // gate), file 1 differs but tokens have identical kind_id/hash →
        // same window_hash. find_raw_clones must reject file 1 via
        // verify_tokens and match file 2 against file 0.
        //
        // Each token points at a distinct 4-byte slice so the window has
        // 100 distinct texts and clears MIN_DISTINCT_TEXTS.
        let make_source = |prefix: u8| -> Vec<u8> {
            (0..MIN_TOKENS)
                .flat_map(|i| {
                    let mut s = vec![prefix];
                    s.extend_from_slice(format!("{i:03}").as_bytes());
                    s
                })
                .collect()
        };
        let make_tokens = |n: usize| -> Vec<Token> {
            (0..n)
                .map(|i| Token {
                    kind_id: 1,
                    start_byte: i * 4,
                    end_byte: i * 4 + 4,
                    line: i + 1,
                    hash: 42 + i as u64,
                })
                .collect()
        };

        let file_data: Vec<Option<FileTokens>> = vec![
            Some(FileTokens {
                source: make_source(b'a'),
                tokens: make_tokens(MIN_TOKENS),
            }),
            Some(FileTokens {
                source: make_source(b'b'),
                tokens: make_tokens(MIN_TOKENS),
            }),
            Some(FileTokens {
                source: make_source(b'a'),
                tokens: make_tokens(MIN_TOKENS),
            }),
        ];

        let raw = find_raw_clones(&file_data);
        // File 2 matches file 0, file 1 matches neither.
        assert_eq!(raw.len(), 1);
        let (rfi, _, _, cfi, _, _) = raw[0];
        assert_eq!(rfi, 2);
        assert_eq!(cfi, 0);
    }

    #[test]
    fn error_subtree_tokens_ignored() {
        let dir = tempfile::tempdir().unwrap();
        // 100+ statements inside a broken syntax context — most tokens
        // land under an ERROR subtree and must be skipped.
        let stmts: String = (1..=25)
            .map(|i| format!("const v_{i} = compute({i}, \"p_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n");
        let broken = format!("function foo( {{\n{stmts}\n}}}}}}");
        let pa = dir.path().join("a.ts");
        std::fs::write(&pa, &broken).unwrap();
        let fa = SourceFile {
            path: pa,
            language: Language::TypeScript,
        };
        let mut parser = Parser::new();
        let ft = tokenize_file(&mut parser, &fa).unwrap();
        assert!(
            ft.tokens.len() < MIN_TOKENS,
            "ERROR subtree tokens should be skipped, got {} tokens",
            ft.tokens.len(),
        );
    }

    #[test]
    fn merge_refuses_non_adjacent_canonical() {
        let files: Vec<SourceFile> = vec![
            make_file("/a.ts", Language::TypeScript),
            make_file("/b.ts", Language::TypeScript),
        ];
        let file_refs: Vec<&SourceFile> = files.iter().collect();

        // Each token points at a distinct 4-byte slice so the merged span
        // has plenty of distinct trigrams and clears the diversity gate.
        let make_ft = |n: usize| -> FileTokens {
            let source: Vec<u8> = (0..n)
                .flat_map(|i| format!("t{i:03}").into_bytes())
                .collect();
            FileTokens {
                source,
                tokens: (0..n)
                    .map(|i| Token {
                        kind_id: 1,
                        start_byte: i * 4,
                        end_byte: i * 4 + 4,
                        line: i + 1,
                        hash: i as u64,
                    })
                    .collect(),
            }
        };
        let file_data: Vec<Option<FileTokens>> = vec![
            Some(make_ft(MIN_TOKENS + 250)),
            Some(make_ft(MIN_TOKENS + 250)),
        ];
        // First run: 50 adjacent raw clones merge into one span covering
        // tokens [0, 49 + MIN_TOKENS - 1]. Long enough to clear the
        // diversity gate's trigram threshold.
        let mut raw: Vec<RawClone> = (0..50)
            .map(|k| (0usize, k, k + 1, 1usize, k, k + 1))
            .collect();
        // Second run: starts after a gap → separate clone, also long enough.
        raw.extend((0..50).map(|k| (0usize, 10 + 50 + k, 20 + 50 + k, 1usize, 50 + k, 80 + k)));
        let diags = merge_and_emit(&mut raw, &file_data, &file_refs);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn vue_files_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let content = "<template><div>hello</div></template><script>const x = 1;</script>";
        let pa = dir.path().join("a.vue");
        let pb = dir.path().join("b.vue");
        std::fs::write(&pa, content).unwrap();
        std::fs::write(&pb, content).unwrap();
        let fa = SourceFile {
            path: pa,
            language: Language::Vue,
        };
        let fb = SourceFile {
            path: pb,
            language: Language::Vue,
        };
        assert!(lint_files(&[&fa, &fb]).is_empty());
    }

    #[test]
    fn clone_line_span_exact() {
        let dir = tempfile::tempdir().unwrap();
        let block = large_ts_block(20);
        let (fa, fb) = write_pair(&dir, "ts", &block);
        let diags = lint_files(&[&fa, &fb]);
        assert_eq!(diags.len(), 1);
        // 20 statements, one per line → exactly 20 lines
        assert!(
            diags[0].message.contains("20 lines"),
            "expected '20 lines', got: {}",
            diags[0].message,
        );
    }

    #[test]
    fn normalize_skips_blanks_and_comments() {
        let dir = tempfile::tempdir().unwrap();
        let pa = dir.path().join("a.ts");
        let content = "// header\nconst x = 1;\n\n// comment\nconst y = 2;\n";
        std::fs::write(&pa, content).unwrap();
        let f = SourceFile {
            path: pa,
            language: Language::TypeScript,
        };
        let mut parser = Parser::new();
        let ft = tokenize_file(&mut parser, &f).unwrap();
        assert!(ft.tokens.len() >= 10);
        for t in &ft.tokens {
            let text = std::str::from_utf8(&ft.source[t.start_byte..t.end_byte]).unwrap();
            assert!(!text.starts_with("//"));
        }
    }

    #[test]
    fn no_false_positive_on_symmetric_suffix_siblings() {
        // Regression test for issue #338.
        // deactivate/reactivate handlers share an identical suffix (CTE + JOIN
        // block) but their setup and UPDATE call differ — the gap is larger
        // than SYMMETRIC_SIBLING_GAP_THRESHOLD.  Name-based suppression must
        // kick in because both files live in the same directory and their stems
        // contain a known complement pair (deactivate / reactivate).
        let dir = tempfile::tempdir().unwrap();

        let suffix = large_ts_block(20); // identical in both files
        let setup_a: String = (1..=15)
            .map(|i| format!("const setupA_{i} = initDeactivate({i}, \"a_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n");
        let setup_b: String = (1..=15)
            .map(|i| format!("const setupB_{i} = initReactivate({i}, \"b_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n");

        let deactivate_content = format!("{setup_a}\n{suffix}");
        let reactivate_content = format!("{setup_b}\n{suffix}");

        let pa = dir.path().join("deactivate-product.ts");
        let pb = dir.path().join("reactivate-product.ts");
        std::fs::write(&pa, &deactivate_content).unwrap();
        std::fs::write(&pb, &reactivate_content).unwrap();
        let fa = SourceFile { path: pa, language: Language::TypeScript };
        let fb = SourceFile { path: pb, language: Language::TypeScript };
        assert!(
            lint_files(&[&fa, &fb]).is_empty(),
            "symmetric suffix siblings (deactivate/reactivate) must not be flagged as clones"
        );
    }

    #[test]
    fn no_false_positive_on_symmetric_siblings() {
        // Regression test for issue #343.
        // Two symmetric sibling handlers (deactivate / reactivate) — identical
        // structure, one load-bearing difference: the argument passed to .set().
        // The files must NOT be flagged as clones.
        let dir = tempfile::tempdir().unwrap();

        // Shared prefix and suffix (20 unique statements each) give the handler
        // enough tokens to exceed MIN_TOKENS on both sides of the diff.
        let prefix = large_ts_block(20);
        let suffix: String = (1..=20)
            .map(|i| format!("const alpha_{i} = processResult({i}, \"item_{i}\");"))
            .collect::<Vec<_>>()
            .join("\n");

        let deactivate = format!(
            "{prefix}\nconst r = db.update(table).set({{ deactivatedAt: sql`coalesce(${{entity.deactivatedAt}}, now())`, updatedAt: sql`now()` }}).returning();\n{suffix}"
        );
        let reactivate = format!(
            "{prefix}\nconst r = db.update(table).set({{ deactivatedAt: null, updatedAt: sql`now()` }}).returning();\n{suffix}"
        );

        let pa = dir.path().join("deactivate.ts");
        let pb = dir.path().join("reactivate.ts");
        std::fs::write(&pa, &deactivate).unwrap();
        std::fs::write(&pb, &reactivate).unwrap();
        let fa = SourceFile { path: pa, language: Language::TypeScript };
        let fb = SourceFile { path: pb, language: Language::TypeScript };
        assert!(
            lint_files(&[&fa, &fb]).is_empty(),
            "symmetric sibling handlers should not be flagged as clones"
        );
    }

    #[test]
    fn low_entropy_boilerplate_not_flagged() {
        // Repeated registration-table boilerplate — many matching tokens but
        // very few distinct identifier-like texts. The diversity gate must
        // reject this even though byte-for-byte verify_tokens succeeds.
        let dir = tempfile::tempdir().unwrap();
        let block: String = (1..=40)
            .map(|_| "(L::T, B::TS(Box::new(m::C))),".to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let wrapped = format!("fn r() -> Vec<X> {{ vec![\n{block}\n] }}");
        let pa = dir.path().join("a.rs");
        let pb = dir.path().join("b.rs");
        std::fs::write(&pa, &wrapped).unwrap();
        std::fs::write(&pb, &wrapped).unwrap();
        let fa = SourceFile {
            path: pa,
            language: Language::Rust,
        };
        let fb = SourceFile {
            path: pb,
            language: Language::Rust,
        };
        let diags = lint_files(&[&fa, &fb]);
        assert!(
            diags.is_empty(),
            "expected no clones for low-entropy boilerplate, got {}: {:?}",
            diags.len(),
            diags.iter().map(|d| &d.message).collect::<Vec<_>>(),
        );
    }
}
