//! Native token-based clone detection (Rabin-Karp).
//!
//! - Parse each file with tree-sitter, extract leaf tokens
//! - Hash sliding windows of MIN_TOKENS tokens
//! - Cross-file collisions with token-by-token verification = clones

use std::collections::HashMap;

use rayon::prelude::*;
use tree_sitter::Parser;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::{Language, SourceFile};
use crate::parsing;

pub const RULE_ID: &str = "no-clones";
const MIN_TOKENS: usize = 100;
const BUCKET_SATURATED: usize = 64;

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
    let mut index: HashMap<u64, Vec<Occurrence>> = HashMap::new();
    let mut raw: Vec<RawClone> = Vec::new();

    for (fi, ft) in file_data.iter().enumerate() {
        let Some(ft) = ft else { continue };
        if ft.tokens.len() < MIN_TOKENS {
            continue;
        }
        for start in 0..=(ft.tokens.len() - MIN_TOKENS) {
            let wh = window_hash(&ft.tokens[start..start + MIN_TOKENS]);

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
                    raw.push((fi, start, ft.tokens[start].line, occ.file_idx, occ.start_token, occ.start_line));
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

    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let (rfi, rstart, rline, cfi, cstart, cline) = raw[i];
        let mut last_rstart = rstart;
        let mut last_cstart = cstart;
        let mut j = i + 1;
        while j < raw.len() {
            let (nrfi, nrstart, _, ncfi, ncstart, _) = raw[j];
            if nrfi == rfi && ncfi == cfi && nrstart == last_rstart + 1 && ncstart == last_cstart + 1 {
                last_rstart = nrstart;
                last_cstart = ncstart;
                j += 1;
                continue;
            }
            break;
        }

        let lines_in_clone = clone_line_span(file_data, rfi, rstart, last_rstart);
        out.push(Diagnostic {
            path: files[rfi].path.clone(),
            line: rline,
            column: 1,
            rule_id: RULE_ID.into(),
            message: format!(
                "Duplicated block ({lines_in_clone} lines) — also in `{}` at line {cline}.",
                files[cfi].path.display(),
            ),
            severity: Severity::Warning,
            span: None,
        });
        i = j;
    }
    out
}

fn clone_line_span(file_data: &[Option<FileTokens>], fi: usize, first_tok: usize, last_window_tok: usize) -> usize {
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
            tokens.push(Token { kind_id, start_byte, end_byte, line, hash });
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
        Language::Vue
        | Language::Toml
        | Language::Json
        | Language::Dockerfile
        | Language::Sql => None,
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
        h = h.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(t.hash);
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
            SourceFile { path: pa, language: lang },
            SourceFile { path: pb, language: lang },
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
        let fa = SourceFile { path: pa, language: Language::TypeScript };
        let fb = SourceFile { path: pb, language: Language::TypeScript };
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
        let fa = SourceFile { path: pa, language: Language::JavaScript };
        let fb = SourceFile { path: pb, language: Language::TypeScript };
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
        let fa = SourceFile { path: pa, language: Language::TypeScript };
        let fb = SourceFile { path: pb, language: Language::TypeScript };
        let diags = lint_files(&[&fa, &fb]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_comments() {
        let dir = tempfile::tempdir().unwrap();
        let with_comments: String = (1..=20)
            .map(|i| format!("// comment {i}\nconst value_{i} = computeExpensive({i}, \"param_{i}\");"))
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
        let fa = SourceFile { path: pa, language: Language::TypeScript };
        let fb = SourceFile { path: pb, language: Language::TypeScript };
        let diags = lint_files(&[&fa, &fb]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn hash_collision_with_first_mismatch() {
        // File 0 and 2 share bytes ("aaaa"), file 1 differs ("bbbb") but
        // tokens have identical kind_id/hash → same window_hash.
        // find_raw_clones must reject file 1 via verify_tokens and match
        // file 2 against file 0.
        let make_tokens = |n: usize| -> Vec<Token> {
            (0..n)
                .map(|i| Token {
                    kind_id: 1,
                    start_byte: 0,
                    end_byte: 4,
                    line: i + 1,
                    hash: 42 + i as u64,
                })
                .collect()
        };

        let file_data: Vec<Option<FileTokens>> = vec![
            Some(FileTokens { source: b"aaaa".to_vec(), tokens: make_tokens(MIN_TOKENS) }),
            Some(FileTokens { source: b"bbbb".to_vec(), tokens: make_tokens(MIN_TOKENS) }),
            Some(FileTokens { source: b"aaaa".to_vec(), tokens: make_tokens(MIN_TOKENS) }),
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
        let fa = SourceFile { path: pa, language: Language::TypeScript };
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

        let make_ft = |n: usize| -> FileTokens {
            FileTokens {
                source: b"x".to_vec(),
                tokens: (0..n).map(|i| Token {
                    kind_id: 1, start_byte: 0, end_byte: 1,
                    line: i + 1, hash: i as u64,
                }).collect(),
            }
        };
        let file_data: Vec<Option<FileTokens>> = vec![
            Some(make_ft(MIN_TOKENS + 50)),
            Some(make_ft(MIN_TOKENS + 80)),
        ];
        let mut raw: Vec<RawClone> = vec![
            //  (rfi, rstart, rline, cfi, cstart, cline)
            (0, 0, 1, 1, 0, 1),
            (0, 1, 2, 1, 1, 2),   // both sides advance → merge
            (0, 10, 20, 1, 50, 80), // gap on both sides → separate
        ];
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
        let fa = SourceFile { path: pa, language: Language::Vue };
        let fb = SourceFile { path: pb, language: Language::Vue };
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
        let f = SourceFile { path: pa, language: Language::TypeScript };
        let mut parser = Parser::new();
        let ft = tokenize_file(&mut parser, &f).unwrap();
        assert!(ft.tokens.len() >= 10);
        for t in &ft.tokens {
            let text = std::str::from_utf8(&ft.source[t.start_byte..t.end_byte]).unwrap();
            assert!(!text.starts_with("//"));
        }
    }
}
