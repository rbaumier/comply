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
//! - A comment documenting the same-named declaration or object/type member in
//!   another file is a parallel API surface (a SIMD vs scalar backend, or a
//!   runtime props object mirrored by a TS type), not a copy, so it is exempt.

use rustc_hash::FxHashSet;
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

/// Tooling directives that occupy a single `//` line (`// eslint-disable-next-line`,
/// `// @ts-expect-error`). A directive is a distinct line, so it both excludes its
/// own group *and* splits a run of `//` lines (see `line_is_marker`) so it never
/// merges into the prose docblock beneath it.
const DIRECTIVE_MARKERS: &[&str] = &[
    "eslint-disable",
    "@ts-",
    "biome-ignore",
    "prettier-ignore",
    "stylelint-disable",
    "tslint:",
    "comply-ignore",
];

/// License/copyright banner phrases. These are duplicated by design across files
/// and can sit on *any* line of a multi-line banner (the Apache header carries
/// `copyright` on line 4), so they exclude the whole merged block but must never
/// fragment it — unlike directives, they do not drive the `line_is_marker` split.
/// `@license` / `@copyright` are the JSDoc tags whose sole purpose is to declare a
/// per-file license/copyright header (the IDux MIT banner opens with `@license`).
const BANNER_MARKERS: &[&str] = &[
    "spdx-license-identifier",
    "copyright",
    "licensed under",
    "licensed to the",
    "@license",
    "@copyright",
    "all rights reserved",
];

/// Recognized open-source license names. A banner that pairs one of these with the
/// word `license`/`licence` is a license header (`MIT-style license`,
/// `BSD-3-Clause license`) even when it carries none of the `BANNER_MARKERS`
/// phrases — the IDux MIT header is exactly this prose shape.
const LICENSE_NAMES: &[&str] =
    &["mit", "apache", "bsd", "isc", "gpl", "lgpl", "agpl", "mpl", "mozilla", "unlicense"];

/// A merged comment block is excluded when it carries any directive or banner
/// marker, is a license header named in prose, or carries a safety marker.
/// Matched by content, not file position, so a banner below `#![attr]` /
/// `'use client'` / a shebang is still caught and a Rust `//!` module doc is not.
///
/// A safety marker (`SAFETY:` / `Safety:` / `safety:` / `safety.` / `# Safety`,
/// per the shared `is_safety_marker` predicate) documents the invariants that
/// make a specific unsafe operation sound; when two unsafe blocks perform the
/// same operation their safety rationale is identical because the invariant is
/// the same. Restating that invariant at every call site is the idiomatic Rust
/// practice (the same marker `rust-undocumented-unsafe` requires), so the
/// repetition is local safety documentation, not a copy-paste smell.
fn is_excluded_comment(lower: &str) -> bool {
    DIRECTIVE_MARKERS.iter().chain(BANNER_MARKERS).any(|m| lower.contains(m))
        || is_named_license_banner(lower)
        || crate::rules::rust_helpers::is_safety_marker(lower)
}

/// A license header that declares its license by name in prose rather than by an
/// SPDX id or `@license` tag — `governed by an MIT-style license`, `released under
/// the BSD license`. Requires a recognized license name *token* (not substring, so
/// `permit`/`commit`/`mozilla-bound` do not match) to co-occur with the word
/// `license`/`licence`, keeping ordinary prose that merely mentions one word out.
///
/// The name+word pairing is deliberately broader than a strict file-top banner: a
/// rare copy-pasted prose comment that names a license is exempted too. That trade
/// is intentional — license wording recurs across files by design far more often
/// than it is genuinely copy-pasted cruft.
fn is_named_license_banner(lower: &str) -> bool {
    let mut has_license_name = false;
    let mut has_license_word = false;
    for token in lower.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()) {
        if LICENSE_NAMES.contains(&token) {
            has_license_name = true;
        } else if matches!(token, "license" | "licence" | "licensed" | "licensing") {
            has_license_word = true;
        }
        if has_license_name && has_license_word {
            return true;
        }
    }
    false
}

/// Module-top string-literal pragmas (`"use client"`, `"use no memo"`, …). A
/// comment trailing one of these on the same physical line is mandated identical
/// across every file carrying the pragma, so it must not be flagged as a
/// near-duplicate.
const PRAGMA_LITERALS: &[&str] = &["use no memo", "use client", "use server", "use strict"];

/// A directive/pragma comment whose text an external tool/framework contract
/// fixes byte-for-byte — so it is the same in every file by design and the
/// "keep one, point the rest at it" remedy cannot apply. Two shapes:
///
/// 1. the comment *is* a tool directive (`/* oxlint-disable … */`,
///    `// eslint-disable-next-line …`): the stripped text starts with a known
///    directive token, per the shared `comment_blocks::is_tool_directive`;
/// 2. the comment *trails* a string-literal pragma statement on the same line
///    (`"use no memo"; // …`): the source before its column is exactly such a
///    statement and nothing else.
fn is_directive_or_pragma_comment(group: &CommentGroup, source: &str) -> bool {
    if crate::rules::comment_blocks::is_tool_directive(&group.stripped) {
        return true;
    }
    line_before_comment(source, group.start_byte).is_some_and(is_pragma_literal_statement)
}

/// The source text on the comment's physical line, up to the comment's start.
/// `None` when the comment is the first thing on the file (no preceding byte).
fn line_before_comment(source: &str, comment_start: usize) -> Option<&str> {
    let line_start = source[..comment_start].rfind('\n').map_or(0, |i| i + 1);
    source.get(line_start..comment_start)
}

/// True when code precedes the comment on its own physical line — the comment
/// *trails* that code (`UInt16(72), // ppi`) rather than standing above it.
///
/// A trailing comment is a per-line label whose scope ends with its line, not a
/// paragraph of prose, so it forms a group by itself: a column of aligned field
/// labels must not be glued into one pseudo-paragraph.
fn is_trailing_comment(source: &str, comment_start: usize) -> bool {
    line_before_comment(source, comment_start).is_some_and(|before| !before.trim().is_empty())
}

/// True when `before` (the source preceding a trailing comment) is exactly a
/// recognized pragma string-literal statement and nothing else: an optional
/// surrounding-whitespace `"use client"` / `'use no memo'`, with an optional
/// trailing `;`. Arbitrary code before the comment is not a pragma, so it does
/// not exempt the trailing comment.
fn is_pragma_literal_statement(before: &str) -> bool {
    let trimmed = before.trim();
    let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
    let inner = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| trimmed.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')));
    inner.is_some_and(|lit| PRAGMA_LITERALS.contains(&lit))
}

/// Source-file extensions that mark a relative path as a citation target.
const SOURCE_EXTS: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".rs"];

/// Cues that introduce a citation to *any* reference — `see X`, `cf X`,
/// `rationale in X`, `convention in X`. These words rarely appear except to
/// point at a canonical source, so they exempt a pointer to a doc or to a
/// sibling source file alike (the latter is what `rationale in head.ts` needs).
const STRONG_CUES: &[&str] =
    &["see", "cf", "voir", "ref", "reference", "referenced", "rationale", "convention"];

/// Verb cues that mark a citation only when the reference is a *documentation*
/// target. `documented in docs/x.md` is a pointer; `defined in app.ts` /
/// `described in cache.ts` is ordinary prose naming where code lives — so those
/// must not exempt a duplicated rationale that merely names a source file.
const WEAK_CUES: &[&str] =
    &["documented", "described", "explained", "defined", "detailed", "noted", "specified"];

/// Connectors allowed between a cue and its reference (`convention in X`,
/// `documented at X`, `convention dans X`). Kept tiny on purpose: a wider window
/// would let a cue word elsewhere in the sentence latch onto an incidental path.
const CITATION_CONNECTORS: &[&str] = &["in", "at", "dans"];

#[derive(Clone, Copy)]
enum CueKind {
    Strong,
    Weak,
}

#[derive(Clone, Copy)]
enum RefKind {
    /// A documentation target: a URL or a `docs/…` / `*.md` path.
    Doc,
    /// A relative source path (`dir/file.ts`).
    Source,
}

/// A "pointer" comment whose job is to cite a single canonical source — a doc,
/// ADR, URL, or sibling file — rather than restate a rationale. Such comments
/// are the deduplication remedy this rule recommends (keep one explanation in a
/// canonical place; let each call site carry a thin pointer to it), so their
/// intentionally near-identical wording is single-source-of-truth done right,
/// not a copy-paste smell.
///
/// Detection is by *adjacency*, matching the "a path introduced by `see` /
/// `rationale in` / …" framing: a reference is a citation only when a cue word
/// sits immediately before it (across at most one connector). Requiring the cue
/// next to the reference — not merely present somewhere — keeps a long
/// duplicated rationale that happens to name a path flagged, and stops a cue
/// word used in ordinary prose (`we see a race in worker/pool.ts`) from
/// exempting it. A weak verb cue only counts against a doc reference, since
/// `defined in app.ts` describes code rather than citing it.
fn is_citation_comment(lower: &str) -> bool {
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    for (j, raw) in tokens.iter().enumerate() {
        let Some(reference) = reference_kind(raw) else {
            continue;
        };
        match (preceding_cue(&tokens, j), reference) {
            (Some(CueKind::Strong), _) | (Some(CueKind::Weak), RefKind::Doc) => return true,
            _ => {}
        }
    }
    false
}

/// The cue introducing the reference at `j`: the token immediately before it,
/// or the one before a single connector (`see X`, `documented at X`).
fn preceding_cue(tokens: &[&str], j: usize) -> Option<CueKind> {
    if j == 0 {
        return None;
    }
    if let Some(kind) = cue_kind(tokens[j - 1]) {
        return Some(kind);
    }
    if j >= 2 && is_connector(tokens[j - 1]) {
        return cue_kind(tokens[j - 2]);
    }
    None
}

/// Classify a whitespace token that names a canonical source: a URL or a
/// `docs/…` / `*.md` doc, versus a relative source path (`dir/file.ts`).
fn reference_kind(raw: &str) -> Option<RefKind> {
    let core = raw.trim_matches(|c: char| {
        !c.is_alphanumeric() && !matches!(c, '/' | '.' | '-' | '_' | ':')
    });
    if core.starts_with("http://") || core.starts_with("https://") {
        return Some(RefKind::Doc);
    }
    let core = core.trim_end_matches('.');
    if core.starts_with("docs/")
        || core.contains("/docs/")
        || core.ends_with(".md")
        || core.ends_with(".mdx")
    {
        return Some(RefKind::Doc);
    }
    if core.contains('/') && SOURCE_EXTS.iter().any(|e| core.ends_with(e)) {
        return Some(RefKind::Source);
    }
    None
}

fn word_core(tok: &str) -> &str {
    tok.trim_matches(|c: char| !c.is_alphanumeric())
}

fn cue_kind(tok: &str) -> Option<CueKind> {
    let word = word_core(tok);
    if STRONG_CUES.contains(&word) {
        Some(CueKind::Strong)
    } else if WEAK_CUES.contains(&word) {
        Some(CueKind::Weak)
    } else {
        None
    }
}

fn is_connector(tok: &str) -> bool {
    CITATION_CONNECTORS.contains(&word_core(tok))
}

/// True when one `//` line is itself a tooling directive. Used to split a run of
/// consecutive line comments so a directive (e.g. `// comply-ignore: …`) never
/// merges into the prose docblock beneath it — otherwise the merged block would
/// inherit the marker and the real comment would be wrongly excluded. Banner
/// phrases are deliberately *not* matched here: a multi-line license header is one
/// logical block, and a banner word mid-block must keep it whole (then
/// `is_excluded_comment` excludes the whole block), never carve it into fragments.
fn line_is_marker(stripped_line: &str) -> bool {
    let lower = stripped_line.to_lowercase();
    DIRECTIVE_MARKERS.iter().any(|m| lower.contains(m))
}

/// The API surface a documented declaration is a member of — the type, trait,
/// class, interface, or module whose block declares it. A file-top-level
/// declaration is a member of none.
///
/// Two declarations in one file that share a surface are siblings: declared on
/// one API surface, they are related by construction rather than by a
/// copy-paste. The surface is the *type*, not the block, so a type whose methods
/// are split across several `impl` blocks (Rust's usual way of varying trait
/// bounds) still reads as one surface.
#[derive(Clone, PartialEq, Eq)]
enum ApiSurface {
    /// A named type, trait, class, interface, or module.
    Named(String),
    /// A member block with no name of its own — a TS type literal written inline
    /// as a return type, or a JS object literal. It exposes no name to serve as
    /// an owner, yet two *distinct* such blocks are as much two distinct owners
    /// as two named interfaces are, so each is identified by its byte offset.
    Unnamed(usize),
}

/// A comment eligible to be compared against others.
struct CommentEntry {
    file_idx: usize,
    line: usize,
    column: usize,
    span: (usize, usize),
    words: Vec<String>,
    /// URL / path tokens the comment cites, in order of appearance. Two comments
    /// that cite *different* targets are two different pointers, however much
    /// prose they share — collapsing them would destroy the mapping each one
    /// records, so the "keep one, cite it" remedy cannot apply.
    references: Vec<String>,
    /// The first `shared_prefix_words` words, joined — the bucket key.
    prefix_key: String,
    /// Name of the declaration or object/type member this comment immediately
    /// documents, if any. Two comments on same-named declarations in different
    /// files are parallel API surfaces — a SIMD and a scalar backend exposing
    /// the same `aes128_decrypt`, or a runtime props object and the TS type
    /// declaring the same prop — so their identical docs are not a smell.
    decl_name: Option<String>,
    /// Name of the type that owns the variant or member this comment documents,
    /// when the documented declaration is an enum variant or a type/interface
    /// member. Two members of two *different* owners that share a name (a
    /// `FlushCompress::None` / `FlushDecompress::None` variant pair, or a
    /// `minimum_amount` field repeated on an OpenAPI request type and its
    /// response type) are parallel API surfaces describing the same concept, so
    /// their identical docs are intentional per-item documentation, not a
    /// copy-paste smell.
    decl_owner: Option<String>,
    /// The API surface this declaration is a member of (see `ApiSurface`).
    decl_surface: Option<ApiSurface>,
    /// Name of the function/method whose body encloses this comment, if any. An
    /// inline body comment inside a same-named method in another file (drizzle's
    /// `queryWithCache` mirrored across its per-dialect session classes)
    /// documents the same step of one algorithm implemented once per package, so
    /// its identical wording is parallel API-surface documentation, not a copy.
    enclosing_decl_name: Option<String>,
}

/// A logical comment block: either one `/* */` node or a run of consecutive
/// `//` lines that share a start column.
struct CommentGroup {
    line: usize,
    column: usize,
    start_byte: usize,
    end_byte: usize,
    stripped: String,
    /// Name of the declaration immediately documented by this block.
    decl_name: Option<String>,
    /// Name of the type owning the variant or member this block documents, if any.
    decl_owner: Option<String>,
    /// The API surface this declaration is a member of (see `ApiSurface`).
    decl_surface: Option<ApiSurface>,
    /// Name of the function/method whose body encloses this block, if any.
    enclosing_decl_name: Option<String>,
    /// Any line of the block has its repetition forced by its enclosing
    /// construct (see `RawComment::cfg_conditional`).
    cfg_conditional: bool,
}

struct RawComment {
    start_byte: usize,
    end_byte: usize,
    row: usize,
    col: usize,
    is_line: bool,
    /// Name of the declaration this comment immediately precedes, captured when
    /// the comment is an outer doc-comment whose next sibling (skipping
    /// attributes) is a named declaration.
    decl_name: Option<String>,
    /// Name of the type owning the variant or member this comment documents, if any.
    decl_owner: Option<String>,
    /// The API surface this declaration is a member of (see `ApiSurface`).
    decl_surface: Option<ApiSurface>,
    /// Name of the function/method whose body encloses this comment, captured by
    /// walking up to the nearest `function_item` / `function_declaration` /
    /// `method_definition` ancestor (see `enclosing_decl_name`).
    enclosing_decl_name: Option<String>,
    /// The comment's repetition is forced by its enclosing construct, so it is
    /// skipped from duplicate detection. Two shapes:
    ///
    /// 1. the documented item compiles only under a `#[cfg(...)]` predicate — the
    ///    comment sits inside a `cfg_if!` macro arm or directly precedes a
    ///    `#[cfg(...)]`-gated item, so the doc is necessarily identical across the
    ///    mutually-exclusive branches that define the same item
    ///    (see `is_cfg_conditional_comment`);
    /// 2. the comment sits inside a `macro_rules!` body — a doc *template* stamped
    ///    onto every item the macro expands, identical by construction and unable
    ///    to be lifted into a canonical doc (see `is_in_macro_definition`).
    ///
    /// Either way the repetition is boilerplate, not copy-paste drift.
    cfg_conditional: bool,
}

/// True when a comment node documents an item that only compiles under a
/// `#[cfg(...)]` predicate. Two mutually-exclusive shapes:
///
/// 1. an ancestor is a `cfg_if!` macro invocation — every arm is gated and at
///    most one compiles, so an item's doc is repeated verbatim across arms;
/// 2. the next sibling is a `#[cfg(...)]` attribute item — the doc belongs to a
///    standalone cfg-gated item, repeated across the parallel gated definitions.
///
/// Only `cfg`/`cfg_attr` attributes count; an ordinary attribute (`#[derive]`,
/// `#[inline]`) after a comment is not a conditional-compilation signal.
fn is_cfg_conditional_comment(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut ancestor = node.parent();
    while let Some(n) = ancestor {
        if n.kind() == "macro_invocation"
            && n.child(0)
                .filter(|c| c.kind() == "identifier")
                .and_then(|c| c.utf8_text(source).ok())
                == Some("cfg_if")
        {
            return true;
        }
        ancestor = n.parent();
    }
    next_named_sibling_is_cfg_attr(node, source)
}

/// True when a comment sits inside a `macro_rules!` definition body — any
/// ancestor is a `macro_definition`. A doc-comment there is a documentation
/// *template* stamped onto every item the macro expands, so it is necessarily
/// identical across every expansion and across an identical macro in a parallel
/// module (nom's `character/streaming.rs` and `complete.rs` each define the same
/// `ints!` macro). It cannot be lifted into a canonical doc or pointed at from
/// elsewhere — it must live inline in the macro arm — so the "keep one, cite the
/// rest" remedy is structurally inapplicable and the repetition is not a smell.
fn is_in_macro_definition(node: tree_sitter::Node) -> bool {
    let mut ancestor = node.parent();
    while let Some(n) = ancestor {
        if n.kind() == "macro_definition" {
            return true;
        }
        ancestor = n.parent();
    }
    false
}

/// The next named sibling, skipping further comments, is a `#[cfg(...)]` /
/// `#[cfg_attr(...)]` attribute item. Walks past intervening comment lines so a
/// multi-line doc run still sees the attribute that follows it.
fn next_named_sibling_is_cfg_attr(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sib = node.next_named_sibling();
    while let Some(n) = sib {
        if is_comment_kind(n.kind()) {
            sib = n.next_named_sibling();
            continue;
        }
        return n.kind() == "attribute_item" && attribute_item_is_cfg(n, source);
    }
    false
}

/// An `attribute_item` whose attribute path is `cfg` or `cfg_attr`.
fn attribute_item_is_cfg(attr_item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = attr_item.walk();
    attr_item.children(&mut cursor).any(|child| {
        child.kind() == "attribute"
            && child
                .child(0)
                .filter(|c| c.kind() == "identifier")
                .and_then(|c| c.utf8_text(source).ok())
                .is_some_and(|name| matches!(name, "cfg" | "cfg_attr"))
    })
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
        // the *first* earlier member that both clears the similarity floor and is
        // not exempt against it — so a bucket of N yields at most N-1
        // diagnostics, and a member's verdict depends only on the bucket's
        // contents, never on which member wins a similarity contest among them.
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
            let reported_against = ordered[..pos].iter().find_map(|&p| {
                let partner = &entries[p];
                // Every member shares the bucket's prefix, so `shared >= prefix_words`.
                let shared = common_prefix_len(&partner.words, &entry.words);
                let shorter = partner.words.len().min(entry.words.len());
                let similar_enough = (shared as f64) >= prefix_pct * (shorter as f64);
                (similar_enough && !is_exempt_pair(entry, partner)).then_some((p, shared))
            });
            let Some((partner_idx, shared)) = reported_against else {
                continue;
            };
            let partner = &entries[partner_idx];
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
                severity: Severity::Error,
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

/// True when two similar comments are documenting parallel constructs rather
/// than repeating one explanation, so the pair must not be reported.
fn is_exempt_pair(entry: &CommentEntry, partner: &CommentEntry) -> bool {
    // Distinct citations: two comments whose reference tokens differ point at two
    // different targets (`/// Source: <url-a>` and `/// Source: <url-b>`, one per
    // ported upstream test). Collapsing them would destroy the mapping each one
    // records, so the remedy this rule proposes cannot apply to them. Only a pair
    // that cites *the same* target — or cites nothing — remains reportable.
    if !entry.references.is_empty()
        && !partner.references.is_empty()
        && entry.references != partner.references
    {
        return true;
    }
    // Parallel API surfaces: a comment documenting the same — or an analogous
    // variant of the same — named declaration or member (a SIMD vs scalar backend
    // both exposing `aes128_decrypt`, a runtime props object and the TS type
    // declaring the same prop, a server `ignore_invalid_headers` and its client
    // `ignore_invalid_headers_in_responses` builder twin, an infallible
    // `add_year_months` and its `add_year_months_opt` twin, or the several
    // overload signatures TypeScript requires one JSDoc block each on) carries the
    // same description because it describes the same item, not because it was
    // copy-pasted.
    if are_parallel_declarations(entry, partner) {
        return true;
    }
    // Parallel inline body documentation: an inline comment inside the body of a
    // same-named function/method in another file documents the same step of an
    // algorithm implemented once per package (drizzle's `queryWithCache` mirrored
    // across its pg/sqlite/gel dialect session classes), so the identical wording
    // describes the same invariant, not a copy-paste smell. Restricted to
    // cross-file matches: an intra-file duplicate, or a comment with no enclosing
    // named function, still flags.
    if entry.file_idx != partner.file_idx
        && enclosing_names_match(
            entry.enclosing_decl_name.as_deref(),
            partner.enclosing_decl_name.as_deref(),
        )
    {
        return true;
    }
    // Implementation twins co-located in one file: an infallible `get_u8` and its
    // fallible `try_get_u8`, or a safe `read` and its `read_ptr` fast path,
    // frequently sit side by side in one trait or module and share a verbatim
    // opening because they perform the same operation. The `a != b` guard is
    // load-bearing: `are_impl_qualifier_twins` treats identical names as trivially
    // equal roots, so without it two same-named declarations in two *different*
    // blocks of one file — which `are_parallel_declarations` deliberately keeps
    // reportable — would slip through here.
    if entry
        .decl_name
        .as_deref()
        .zip(partner.decl_name.as_deref())
        .is_some_and(|(a, b)| a != b && are_impl_qualifier_twins(a, b))
    {
        return true;
    }
    // Parallel members of distinct owners: two enum variants of *different* enums
    // (`FlushCompress::None` / `FlushDecompress::None` mirroring the same zlib
    // flush mode for the two directions), or two type/interface members of
    // *different* types that share a name (an OpenAPI `minimum_amount` field
    // carried by both a request type and its response type) describe the same
    // concept, so their identical doc-comments are intentional per-item
    // documentation. A member name is unique within its owner, so requiring
    // distinct owners means a same-file copy-paste inside one type can never
    // qualify; this exemption therefore applies regardless of file.
    are_parallel_owned_members(entry, partner)
}

/// Minimum `_`-separated segments the shorter name must carry for a prefix
/// relationship to count as a parallel-API variant pair. A multi-segment root
/// (`ignore_invalid_headers`) shared between `ignore_invalid_headers` and
/// `ignore_invalid_headers_in_responses` is a strong signal of mirrored builder
/// methods; a one- or two-segment root (`init` ⊂ `init_db`) is too generic and
/// could just be two unrelated functions sharing a copy-pasted doc.
const MIN_VARIANT_ROOT_SEGMENTS: usize = 3;

/// Name segments that mark one of two parallel implementations of a single
/// algorithm — a safe/unsafe twin, a pointer/slice twin, or a SIMD/scalar twin.
/// A Rust performance crate routinely ships the same algorithm twice (a checked
/// path and an unchecked-indexing fast path), naming the two by appending one of
/// these qualifiers to a shared root (`read_integer` / `read_integer_ptr`,
/// `duplicate` / `duplicate_slice`). Both carry the same algorithm-description
/// doc because they implement the same steps, so the repetition is intentional,
/// not a copy-paste smell.
const IMPL_QUALIFIER_SEGMENTS: &[&str] = &[
    "safe", "unsafe", "checked", "unchecked", "ptr", "slice", "simd", "scalar", "generic",
    // Streaming/complete-mode direction qualifier: byte-parser crates (nom,
    // winnow, …) ship the same operation twice, the `_complete` variant treating
    // end-of-input as success rather than `Incomplete` (`parse` / `parse_complete`).
    "complete",
    // Big-endian / little-endian direction qualifier: binary-parsing crates (nom,
    // byteorder, bytes) ship the same byte parser twice as `be_`/`le_` variants;
    // for a 1-byte integer both directions read identically, so their docs match
    // by design (`be_u8` / `le_u8`, `be_i8` / `le_i8`).
    "be", "le",
];

/// Two doc-comments document parallel API surfaces when both name a declaration
/// and the names are either identical or analogous variants: one a clean
/// `_`-boundary prefix of the other (`ignore_invalid_headers` ⊂
/// `ignore_invalid_headers_in_responses`, the server/client builder twins that
/// differ only by a `_in_responses` suffix), a Rust impl-direction qualifier twin
/// (`read_integer` / `read_integer_ptr`), or a camelCase sync/async twin
/// (`parse` / `parseAsync`). The shared root must span at least
/// `MIN_VARIANT_ROOT_SEGMENTS` segments so a short generic prefix never collapses
/// two unrelated copy-pasted docs into one exempt pair.
fn are_parallel_decl_names(a: Option<&str>, b: Option<&str>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    a == b
        || is_variant_suffix_of(a, b)
        || is_variant_suffix_of(b, a)
        || are_impl_qualifier_twins(a, b)
        || are_async_sync_twins(a, b)
}

/// Two *distinct* names that mark analogous variants of one operation, with the
/// `MIN_VARIANT_ROOT_SEGMENTS` floor lifted (`find` / `find_at`, `new` /
/// `new_no_color`, `build` / `build_many`). The floor exists to rule out two
/// *unrelated* declarations that happen to share a short generic prefix; when the
/// two are declared on one API surface that possibility is already excluded by
/// their sibling membership, so the floor has nothing left to guard.
///
/// The fallible `try_` prefix is stripped before the roots are compared, so it
/// composes with the suffix relation the way the names themselves do:
/// `try_find_iter` is `find_iter`'s fallible form and `find_iter_at` is its
/// positioned form, which makes the two variants of one operation.
fn are_variant_names(a: &str, b: &str) -> bool {
    if a == b {
        return false;
    }
    let (a_root, b_root) = (strip_fallible_prefix(a), strip_fallible_prefix(b));
    a_root == b_root
        || is_suffixed_variant(a_root, b_root)
        || is_suffixed_variant(b_root, a_root)
        || are_impl_qualifier_twins(a, b)
        || are_async_sync_twins(a, b)
}

/// `name` without the leading `try_` that marks Rust's fallible twin.
fn strip_fallible_prefix(name: &str) -> &str {
    name.strip_prefix("try_").unwrap_or(name)
}

/// Two doc-comments document parallel API surfaces when both name a declaration
/// and the two declarations are separate entry points rather than one
/// explanation pasted twice.
///
/// Across files this is the name relation alone: two files are two API surfaces
/// by construction (a sync crate and its async twin, a SIMD backend and its
/// scalar one).
///
/// Within one file the two declarations must additionally share a *scope*, since
/// that is what makes a name match evidence of one item rather than of a
/// copy-paste:
///
/// - the same name in one scope is one item declared several times — the several
///   overload signatures TypeScript requires one JSDoc block each on, or a trait
///   method contract restated on the item that implements it. There is no syntax
///   for sharing one block across them, so the "keep one, cite it" remedy has no
///   form. Two same-named declarations in *different* scopes of one file (two
///   classes each declaring `clearDatabase`) stay reportable;
/// - variant names in one member block are siblings of one API surface —
///   `Matcher::find` and `Matcher::find_at`, `Builder::new` and
///   `Builder::new_no_color` — each of which rustdoc renders as its own page and
///   so must document itself. Sibling membership is directly observable, so the
///   root-length floor is waived for it;
/// - at file top level there is no block to observe, so the floor stands: two
///   unrelated free functions sharing a generic prefix keep flagging.
///
/// A comment with no `decl_name` (a free-floating rationale, or a plain `//`
/// above a top-level declaration) never qualifies, so a copy-pasted rationale
/// stays in scope wherever it sits.
fn are_parallel_declarations(entry: &CommentEntry, partner: &CommentEntry) -> bool {
    let (Some(a), Some(b)) = (entry.decl_name.as_deref(), partner.decl_name.as_deref()) else {
        return false;
    };
    if entry.file_idx != partner.file_idx {
        return are_parallel_decl_names(Some(a), Some(b));
    }
    if entry.decl_surface != partner.decl_surface {
        return false;
    }
    if entry.decl_surface.is_some() && are_variant_names(a, b) {
        return true;
    }
    are_parallel_decl_names(Some(a), Some(b))
}

/// Two inline comments share an enclosing API surface when both sit inside a
/// named function/method and the two names are identical. Same-named
/// functions/methods in different files are parallel implementations of one API
/// (a per-dialect session `queryWithCache`), so an inline comment restating the
/// same step carries identical wording by necessity. A `None` enclosing name
/// (the comment sits inside no named function) never matches, so a free-floating
/// duplicate still flags.
fn enclosing_names_match(a: Option<&str>, b: Option<&str>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a == b)
}

/// Two declaration names are parallel-implementation twins when one is the
/// fallible `try_`-prefixed variant of the other (`get_u8` / `try_get_u8`,
/// `copy_to_slice` / `try_copy_to_slice`), or when they reduce to the same root
/// after stripping one leading/trailing implementation-direction qualifier
/// segment (`read_integer` / `read_integer_ptr`, `duplicate` / `duplicate_slice`,
/// `decode_simd` / `decode_scalar`). The shared root must still carry a segment,
/// so a bare `ptr` / `safe` collapsing to nothing is not a twin. Because the
/// names *differ*, this can only ever pair two distinct declarations — an
/// exact-copy of one name onto itself stays caught by the `a == b` cross-file
/// path, and an intra-file copy-paste does not become two differently-qualified
/// names.
///
/// Unlike `is_variant_suffix_of`, a one-segment shared root (`run_safe` /
/// `run_unsafe` → `run`) is accepted: the discriminating signal here is the
/// closed, semantically-loaded qualifier vocabulary, not the length of the root.
/// A `_safe`/`_ptr` segment marks a deliberate implementation direction, whereas
/// the arbitrary `_in_responses`-style suffix the variant path handles needs a
/// long root to rule out two unrelated functions sharing a generic prefix.
fn are_impl_qualifier_twins(a: &str, b: &str) -> bool {
    if is_fallible_twin(a, b) {
        return true;
    }
    let root_a = strip_impl_qualifier(a);
    let root_b = strip_impl_qualifier(b);
    root_a == root_b && !root_a.is_empty()
}

/// True when one name is the other prefixed with `try_` — the canonical Rust
/// fallible/infallible twin idiom (`get_u8` / `try_get_u8`). Checked as a
/// whole-name relationship rather than through `strip_impl_qualifier` so it holds
/// even when the shared root itself ends in a qualifier segment (`copy_to_slice` /
/// `try_copy_to_slice`, where stripping `slice` independently from each side would
/// leave mismatched roots). The `_` in the `try_` prefix keeps the match on a
/// segment boundary, so a bare `tryout` never pairs with `out`.
fn is_fallible_twin(a: &str, b: &str) -> bool {
    a.strip_prefix("try_") == Some(b) || b.strip_prefix("try_") == Some(a)
}

/// `name` with one recognized implementation-direction qualifier removed from a
/// `_`-boundary at either end, e.g. `read_integer_ptr` → `read_integer`,
/// `unsafe_copy` → `copy`. Returns `name` unchanged when it carries no such
/// qualifier. Only one qualifier is stripped, so the remaining root keeps any
/// other distinguishing segments.
fn strip_impl_qualifier(name: &str) -> &str {
    for q in IMPL_QUALIFIER_SEGMENTS {
        if let Some(root) = name.strip_suffix(q).and_then(|r| r.strip_suffix('_')) {
            return root;
        }
        if let Some(root) = name.strip_prefix(q).and_then(|r| r.strip_prefix('_')) {
            return root;
        }
    }
    name
}

/// Two declaration names are sync/async twins when they reduce to the same base
/// after stripping a trailing camelCase `Async` or `Sync` suffix — the
/// JavaScript/TypeScript convention for exposing one operation in both
/// synchronous and asynchronous form (`parse` / `parseAsync`,
/// `safeParse` / `safeParseAsync`, `parseSync` / `parseAsync`). Both variants
/// carry the same JSDoc because they describe the same operation, so the
/// repetition is intentional. This is the camelCase counterpart of the
/// `_async` / `_sync` `_`-boundary handling `strip_impl_qualifier` gives Rust
/// names. Because the names *differ*, at least one must carry the suffix, so two
/// unrelated non-suffixed names can never pair.
fn are_async_sync_twins(a: &str, b: &str) -> bool {
    a != b && strip_async_sync_suffix(a) == strip_async_sync_suffix(b)
}

/// `name` with a trailing camelCase `Async` or `Sync` suffix removed, when that
/// suffix starts at an uppercase word boundary and leaves a non-empty prefix
/// (`parseAsync` → `parse`, `safeParseSync` → `safeParse`). Returns `name`
/// unchanged otherwise: the uppercase `A`/`S` is the word boundary, so a name
/// ending in lowercase `async` / `sync` (`resync`) never strips, and a bare
/// `Async` / `Sync` (empty prefix) is left intact.
fn strip_async_sync_suffix(name: &str) -> &str {
    for suffix in ["Async", "Sync"] {
        if let Some(root) = name.strip_suffix(suffix).filter(|r| !r.is_empty()) {
            return root;
        }
    }
    name
}

/// Two doc-comments document parallel members of distinct owners when both name
/// a member (each carries an owner type), the member names match as a
/// parallel-API pair, and the *owning types differ*. Distinct owners are what
/// makes this safe to apply within a single file: a member name is unique inside
/// its owner, so two matching member docs can only ever come from two different
/// owners — mirrored directions of one concept (`FlushCompress::None` /
/// `FlushDecompress::None`) or the same field on parallel request/response types
/// (`Restrictions.minimum_amount` on a create-params type and its response type),
/// never a botched copy-paste of one member onto a sibling within one owner.
fn are_parallel_owned_members(a: &CommentEntry, b: &CommentEntry) -> bool {
    owners_are_distinct(a, b)
        && are_parallel_decl_names(a.decl_name.as_deref(), b.decl_name.as_deref())
}

/// The two comments document members of two *different* owners.
///
/// A named owner is compared by name. When neither side has one, the only owner
/// that still counts is an unnamed member block: `omit(keys)` and
/// `omit(keys, message)` each return an inline `{ … }` carrying the same
/// documented member, and two distinct literals are two distinct owners exactly
/// as two named interfaces are. Every *named* owner-less shape — a class body, a
/// Rust `impl` or `mod` body — deliberately stays undecided, so a same-named
/// member declared under two of them keeps flagging.
fn owners_are_distinct(a: &CommentEntry, b: &CommentEntry) -> bool {
    match (a.decl_owner.as_deref(), b.decl_owner.as_deref()) {
        (Some(a_owner), Some(b_owner)) => a_owner != b_owner,
        (None, None) => matches!(
            (&a.decl_surface, &b.decl_surface),
            (Some(ApiSurface::Unnamed(a_block)), Some(ApiSurface::Unnamed(b_block)))
                if a.file_idx != b.file_idx || a_block != b_block
        ),
        _ => false,
    }
}

/// `root` is `name` with a trailing `_<suffix>` appended at a `_` boundary, and
/// `root` itself spans enough segments to be a distinctive shared root.
fn is_variant_suffix_of(root: &str, name: &str) -> bool {
    is_suffixed_variant(root, name)
        && root.split('_').filter(|s| !s.is_empty()).count() >= MIN_VARIANT_ROOT_SEGMENTS
}

/// `root` is `name` with a trailing `_<suffix>` appended at a `_` boundary
/// (`captures_iter` ⊂ `captures_iter_at`), with no requirement on the root's
/// length.
fn is_suffixed_variant(root: &str, name: &str) -> bool {
    name.strip_prefix(root).is_some_and(|rest| rest.starts_with('_'))
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
        if group.cfg_conditional {
            continue;
        }
        let lower = group.stripped.to_lowercase();
        if is_excluded_comment(&lower)
            || is_citation_comment(&lower)
            || is_directive_or_pragma_comment(&group, &source)
        {
            continue;
        }
        let words = normalize_words(&group.stripped);
        if is_all_numeric_label(&words) {
            continue;
        }
        if words.len() < need {
            continue;
        }
        let prefix = &words[..prefix_words];
        if !prefix_passes_entropy(prefix) {
            continue;
        }
        let prefix_key = prefix.join("\u{1}");
        let references = group
            .stripped
            .split_whitespace()
            .filter_map(reference_token)
            .map(str::to_lowercase)
            .collect();
        entries.push(CommentEntry {
            file_idx,
            line: group.line,
            column: group.column,
            span: (group.start_byte, group.end_byte - group.start_byte),
            words,
            references,
            prefix_key,
            decl_name: group.decl_name,
            decl_owner: group.decl_owner,
            decl_surface: group.decl_surface,
            enclosing_decl_name: group.enclosing_decl_name,
        });
    }
    entries
}

/// Declaration node kinds (Rust + TS/JS) that carry a `name` field and can be
/// the subject of a leading doc-comment. A comment whose next sibling is one of
/// these documents that named item.
///
/// A callable is spelled two ways in both grammars — with a body and as a bare
/// signature — and both spellings declare the same API item, so both count. Rust
/// pairs `function_item` with `function_signature_item` / `associated_type`; TS
/// pairs `function_declaration` / `method_definition` with `function_signature`,
/// `method_signature`, and `abstract_method_signature`. That is what makes the
/// same-named-item exemption reach a trait method mirrored by parallel sync and
/// async crates (`embedded-hal` vs `embedded-hal-async`), and an interface
/// method mirrored by the class that implements it.
///
/// A class property is spelled `public_field_definition` — the grammar's only
/// class-property kind, covering `static`, `readonly`, `declare`, `accessor` and
/// `#private` fields alike. It belongs here rather than in `is_named_member`
/// because a class property is documented with a JSDoc block, so it must keep
/// the doc-comment gate; that is what lets an interface's `property_signature`
/// and the class field implementing it read as one documented item.
fn is_named_declaration(kind: &str) -> bool {
    matches!(
        kind,
        // Rust
        "function_item"
            | "function_signature_item"
            | "struct_item"
            | "enum_item"
            | "trait_item"
            | "type_item"
            | "associated_type"
            | "const_item"
            | "static_item"
            | "mod_item"
            | "union_item"
            | "macro_definition"
            // TS / JS
            | "function_declaration"
            | "generator_function_declaration"
            | "function_signature"
            | "class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration"
            | "method_definition"
            | "method_signature"
            | "abstract_method_signature"
            | "public_field_definition"
            | "abstract_class_declaration"
    )
}

/// True when the raw comment bytes carry a doc-comment marker (`///`, `//!`,
/// `/**`, `/*!`). Only doc-comments describe the item they precede; a plain `//`
/// / `/*` comment above a declaration is incidental prose, so a copy-pasted one
/// is still a smell and must not earn the parallel-implementation exemption.
fn is_doc_comment(raw: &[u8]) -> bool {
    raw.starts_with(b"///")
        || raw.starts_with(b"//!")
        || raw.starts_with(b"/**")
        || raw.starts_with(b"/*!")
}

/// Object-literal / type-literal member kinds, plus Rust struct fields, whose
/// name is the key documented by a leading comment. A JS runtime props object
/// (`{ /* doc */ foo: … }`) and the matching TS type field
/// (`type Props = { /* doc */ foo?: … }`) mirror the same prop API, and a Rust
/// `field_declaration` repeated under two parallel iterator structs documents
/// the same backing field, so their identical per-member docs are intentional
/// parallel documentation, not a copy. Per-member docs are conventionally a
/// plain `//` (not `///` / `/**`), so unlike top-level declarations these earn
/// the same-named exemption from any comment — see `documented_decl_name`.
///
/// A method-shaped member (`method_signature`, `abstract_method_signature`) is
/// not here: it is a callable, documented by convention with a JSDoc block, so it
/// goes through `is_named_declaration` and keeps the doc-comment gate.
fn is_named_member(kind: &str) -> bool {
    matches!(kind, "pair" | "property_signature" | "field_declaration")
}

/// The name a member node exposes to a leading comment: the `key` of an
/// object-literal `pair`, the `name` of a TS `property_signature`, or the `name`
/// (a `field_identifier`) of a Rust `field_declaration`. Only an
/// identifier/string key counts — a computed key (`[expr]: …`) names nothing
/// stable to mirror across files.
fn member_name(member: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let key = member
        .child_by_field_name("key")
        .or_else(|| member.child_by_field_name("name"))?;
    if !matches!(key.kind(), "property_identifier" | "string" | "identifier" | "field_identifier") {
        return None;
    }
    key.utf8_text(source).ok().map(str::to_owned)
}

/// What a comment documents, as resolved from the declaration it precedes.
struct DocumentedDecl {
    /// Name of the declaration or member.
    name: Option<String>,
    /// Name of the type that owns it, when it is a member of a named type.
    owner: Option<String>,
    /// The API surface it is a member of, when it is not at file top level.
    surface: Option<ApiSurface>,
}

impl DocumentedDecl {
    /// The comment documents nothing the rule can attribute.
    const NOTHING: Self = Self { name: None, owner: None, surface: None };
}

/// The name of the declaration or member a comment immediately documents, plus
/// the owning type's name when that declaration is an enum variant, a
/// type/interface member, a Rust struct field, or a Rust trait-body item (an
/// associated type or trait method, owned by its enclosing `trait`), plus the
/// member block it belongs to. Looks at the
/// comment's next named sibling, skipping attributes, decorators, and intervening
/// non-doc comments (a `// @ts-expect-error` directive, or any plain `//` note) — such a
/// line between a doc block and its declaration must not drop the attribution. A
/// following *doc*-comment block is not skipped: it, not the block above it,
/// documents the declaration.
/// `DocumentedDecl::NOTHING` for free-floating comments (no following
/// declaration) or declarations without a recognizable name.
///
/// A top-level declaration or enum variant is only documented by a *doc*-comment
/// (`///`, `/**`, …) — a plain `//` above it is incidental prose, so a
/// copy-pasted one is still a smell. Object/type members and Rust struct fields
/// are an exception: their docs are conventionally a plain `//` above the field,
/// so a plain `//` above a member still names it. The owner is returned for enum
/// variants, TS interface/type members, Rust struct fields, and Rust trait-body
/// items (via `enclosing_named_type`, which yields `None` for a top-level
/// declaration); it lets the caller treat same-named members of *different* owners
/// as parallel API surfaces while a botched copy-paste within one owner
/// (impossible — member names are unique per owner) cannot slip through.
fn documented_decl_name(comment: tree_sitter::Node, source: &[u8]) -> DocumentedDecl {
    let Some(mut sibling) = comment.next_named_sibling() else {
        return DocumentedDecl::NOTHING;
    };
    while matches!(sibling.kind(), "attribute_item" | "decorator")
        || (is_comment_kind(sibling.kind())
            && !is_doc_comment(&source[sibling.start_byte()..sibling.end_byte()]))
    {
        let Some(next) = sibling.next_named_sibling() else {
            return DocumentedDecl::NOTHING;
        };
        sibling = next;
    }
    // A TS/JS `export function parse() {}` nests the `function_declaration` under
    // an `export_statement`, and `export declare function parse(): void` nests it
    // one level deeper still, under an `ambient_declaration`. Descend through both
    // wrappers so the name is read from `parse`, not lost at the wrapper.
    if let Some(inner) = unwrap_declaration(sibling) {
        sibling = inner;
    }
    if is_named_member(sibling.kind()) {
        return DocumentedDecl {
            name: member_name(sibling, source),
            owner: enclosing_named_type(sibling, source),
            surface: api_surface(sibling, source),
        };
    }
    let is_doc = is_doc_comment(&source[comment.start_byte()..comment.end_byte()]);
    if is_doc && sibling.kind() == "enum_variant" {
        let name = sibling
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_owned);
        return DocumentedDecl {
            name,
            owner: enclosing_enum_name(sibling, source),
            surface: api_surface(sibling, source),
        };
    }
    if !is_doc || !is_named_declaration(sibling.kind()) {
        return DocumentedDecl::NOTHING;
    }
    // A computed key (`[expr](): …`, reachable on a `method_signature` or a
    // `method_definition`) names nothing stable to mirror across files — its text
    // is the expression, and two unrelated methods both keyed `[key]` would read
    // as the same name. This is the guard `member_name` applies on the member path.
    let name = sibling
        .child_by_field_name("name")
        .filter(|n| n.kind() != "computed_property_name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_owned);
    // A trait-body item is owned by its enclosing `trait`, and a method signature
    // by its enclosing `interface` or `type` alias; a top-level declaration
    // transits to an unnamed container and resolves to `None`, so same-named
    // members of two owners become a parallel-owned pair while a top-level
    // copy-paste stays owner-less.
    DocumentedDecl {
        name,
        owner: enclosing_named_type(sibling, source),
        surface: api_surface(sibling, source),
    }
}

/// The declaration hidden inside a TS wrapper node, or `None` when `node` is not
/// a wrapper. `export function f() {}` wraps the `function_declaration` in an
/// `export_statement` (reachable through its `declaration` field);
/// `declare function f(): void` wraps it in an `ambient_declaration`, which
/// carries no fields at all — its declaration is simply its first named child of
/// a declaration kind. `export declare function f(): void` nests both.
fn unwrap_declaration(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let inner = match node.kind() {
        "export_statement" => node.child_by_field_name("declaration")?,
        "ambient_declaration" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).find(|c| is_named_declaration(c.kind()))?
        }
        _ => return None,
    };
    // `export declare function f()` needs both unwraps, in that order.
    Some(unwrap_declaration(inner).unwrap_or(inner))
}

/// Member-block kinds: the node whose children are the members of one type,
/// trait, impl, module, or object. A declaration whose parent (past the
/// `export`/`declare` wrappers) is one of these is a member of that block; a
/// file-top-level declaration's parent is the file node, so it has no block.
const MEMBER_BLOCK_KINDS: &[&str] = &[
    // TS / JS
    "class_body",
    "interface_body",
    "object_type",
    "object",
    // Rust
    "declaration_list",
    "field_declaration_list",
    "enum_variant_list",
];

/// The `ApiSurface` `decl` is a member of, or `None` when it sits at file top
/// level. The surface is read from the block's *owner* — the `impl`'s type, the
/// `class`'s name — so the several `impl` blocks a Rust type is usually split
/// across resolve to one surface.
fn api_surface(decl: tree_sitter::Node, source: &[u8]) -> Option<ApiSurface> {
    let mut block = decl.parent()?;
    while matches!(block.kind(), "export_statement" | "ambient_declaration") {
        block = block.parent()?;
    }
    if !MEMBER_BLOCK_KINDS.contains(&block.kind()) {
        return None;
    }
    let unnamed = || ApiSurface::Unnamed(block.start_byte());
    let Some(owner) = block.parent() else {
        return Some(unnamed());
    };
    // An `impl` names its surface by the type it is implemented on; every other
    // block-carrying construct names it directly.
    let name = match owner.kind() {
        "impl_item" => owner.child_by_field_name("type").and_then(base_type_name),
        _ => owner.child_by_field_name("name"),
    };
    Some(
        name.and_then(|n| n.utf8_text(source).ok())
            .map_or_else(unnamed, |text| ApiSurface::Named(text.to_owned())),
    )
}

/// The bare type name inside a possibly-generic type node — `Summary` from
/// `Summary<NoColor<W>>`, by descending the leftmost `type` field until a plain
/// identifier is reached.
fn base_type_name(mut node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    while !matches!(node.kind(), "type_identifier" | "identifier" | "scoped_type_identifier") {
        node = node.child_by_field_name("type")?;
    }
    Some(node)
}

/// The name of the `enum_item` enclosing a variant node, via its
/// `enum_variant_list` parent. `None` when the variant is not inside a named
/// enum (defensive — the Rust grammar always nests a variant under one).
fn enclosing_enum_name(variant: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let enum_item = variant.parent()?.parent()?;
    if enum_item.kind() != "enum_item" {
        return None;
    }
    enum_item
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_owned)
}

/// The name of the TS `interface_declaration` / `type_alias_declaration` or the
/// Rust `struct_item` / `trait_item` whose body holds `member`, walking up past
/// the intervening `object_type` / `interface_body` / `field_declaration_list` /
/// `declaration_list`. `None` for a member that has no stable named owner — an
/// object-literal `pair`, a type literal nested inline rather than bound to a
/// named type alias, or a `declaration_list` item whose container is unnamed: an
/// `impl_item` / `mod_item` is not in the named-owner arm, so its members transit
/// the `declaration_list` and then resolve to `None`. Without a named owner two
/// same-named members cannot be proven parallel, so they stay eligible to flag
/// (the cross-file member exemption still covers the named-prop-API-mirror case
/// separately).
fn enclosing_named_type(member: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut node = member.parent();
    while let Some(n) = node {
        match n.kind() {
            "interface_declaration" | "type_alias_declaration" | "struct_item" | "trait_item" => {
                return n
                    .child_by_field_name("name")
                    .and_then(|name| name.utf8_text(source).ok())
                    .map(str::to_owned);
            }
            "object_type" | "interface_body" | "field_declaration_list" | "declaration_list" => {
                node = n.parent();
            }
            _ => return None,
        }
    }
    None
}

/// The name of the nearest function/method whose body encloses `comment`, found
/// by walking up the ancestor chain to the first `function_item` (Rust),
/// `function_declaration`, or `method_definition` (TS/JS) and reading its `name`
/// field. `None` when no such ancestor exists — e.g. a top-level comment, or one
/// inside a closure or class body with no named-function ancestor above it (a
/// closure nested inside a named method still resolves to that method's name).
///
/// A doc-comment that *precedes* a declaration is its sibling, not its
/// descendant, so it is never enclosed by that declaration; this captures only
/// comments sitting *inside* a function body. Two such inline comments inside
/// same-named methods in different files (a caching protocol implemented once
/// per dialect package) describe the same algorithm step, so their identical
/// wording is parallel documentation — the same parallelism the same-named
/// declaration exemption grants doc-comments.
fn enclosing_decl_name(comment: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut ancestor = comment.parent();
    while let Some(n) = ancestor {
        if matches!(n.kind(), "function_item" | "function_declaration" | "method_definition") {
            return n
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
                .map(str::to_owned);
        }
        ancestor = n.parent();
    }
    None
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
            let documented = documented_decl_name(node, source);
            out.push(RawComment {
                start_byte: start,
                end_byte: end,
                row: node.start_position().row,
                col: node.start_position().column,
                is_line,
                decl_name: documented.name,
                decl_owner: documented.owner,
                decl_surface: documented.surface,
                enclosing_decl_name: enclosing_decl_name(node, source),
                // A doc-comment inside a `macro_rules!` body is a template that
                // cannot be lifted out, so it is skipped through the same guard
                // as conditionally-compiled docs (see `cfg_conditional`).
                cfg_conditional: is_cfg_conditional_comment(node, source)
                    || is_in_macro_definition(node),
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
            // The documented declaration is attached to the last line of the
            // run (the one directly above the declaration).
            let mut decl_name = c.decl_name.clone();
            let mut decl_owner = c.decl_owner.clone();
            let mut decl_surface = c.decl_surface.clone();
            let mut texts = vec![first_text.to_string()];
            let mut cfg_conditional = c.cfg_conditional;
            let mut j = i + 1;
            // A trailing comment labels the code on its own line, so it never
            // joins a run: its scope ends with its line. Starting the run on one
            // closes the group immediately.
            let starts_on_trailing = is_trailing_comment(source, c.start_byte);
            while let Some(n) = raws.get(j).filter(|_| !starts_on_trailing) {
                if !(n.is_line && n.col == col && n.row == last_row + 1) {
                    break;
                }
                let n_text = strip_line_marker(&source[n.start_byte..n.end_byte]);
                // A directive line and a prose line never share a block.
                if line_is_marker(n_text) != group_is_marker {
                    break;
                }
                // Column-aligned trailing labels satisfy the same-column,
                // next-row test as well as a prose run does, so the boundary has
                // to be tested explicitly or a column of one-word field names
                // merges into a pseudo-paragraph long enough to clear `min_words`.
                if is_trailing_comment(source, n.start_byte) {
                    break;
                }
                texts.push(n_text.to_string());
                cfg_conditional |= n.cfg_conditional;
                last_row = n.row;
                end_byte = n.end_byte;
                decl_name = n.decl_name.clone();
                decl_owner = n.decl_owner.clone();
                decl_surface = n.decl_surface.clone();
                j += 1;
            }
            groups.push(CommentGroup {
                line: c.row + 1,
                column: c.col + 1,
                start_byte: c.start_byte,
                end_byte,
                stripped: texts.join(" "),
                decl_name,
                decl_owner,
                decl_surface,
                // Every line of a contiguous `//` run shares one enclosing scope,
                // so the first line's enclosing function names the whole block.
                enclosing_decl_name: c.enclosing_decl_name.clone(),
                cfg_conditional,
            });
            i = j;
        } else {
            groups.push(CommentGroup {
                line: c.row + 1,
                column: c.col + 1,
                start_byte: c.start_byte,
                end_byte: c.end_byte,
                stripped: strip_block(&source[c.start_byte..c.end_byte]),
                decl_name: c.decl_name.clone(),
                decl_owner: c.decl_owner.clone(),
                decl_surface: c.decl_surface.clone(),
                enclosing_decl_name: c.enclosing_decl_name.clone(),
                cfg_conditional: c.cfg_conditional,
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

/// The comment's words, lowercased, with each URL or path kept whole.
///
/// A reference is one thing the comment names, so it counts as one word. Split on
/// every non-alphanumeric character instead, a URL becomes a dozen tokens of
/// which only the last one or two carry the identity — and since similarity here
/// is measured on the shared *opening*, any family of references under one base
/// then scores near 1.0 whatever they actually point at.
fn normalize_words(stripped: &str) -> Vec<String> {
    let mut words = Vec::new();
    for token in stripped.split_whitespace() {
        match reference_token(token) {
            Some(reference) => words.push(reference.to_lowercase()),
            None => words.extend(
                token
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .map(str::to_lowercase),
            ),
        }
    }
    words
}

/// The URL or path inside a whitespace-separated token, stripped of the
/// punctuation that surrounds it in prose. `None` when the token names no
/// reference. Shares `reference_kind`'s classification so "what is one word" and
/// "what is a citation target" cannot drift apart.
fn reference_token(raw: &str) -> Option<&str> {
    let core = raw.trim_matches(|c: char| {
        !c.is_alphanumeric() && !matches!(c, '/' | '.' | '-' | '_' | ':')
    });
    reference_kind(raw).map(|_| core.trim_end_matches('.'))
}

/// A comment whose normalized words are *every one* a purely numeric integer
/// token — a byte lookup table's column-index header (`//  0  1  2 … 15`) or a
/// row of magic constants (`// 100 200 300 …`). Such a comment labels a data
/// structure by index, carrying no prose rationale that can drift out of sync,
/// so it can never be the copy-paste smell this rule targets however many tables
/// across however many files share the layout. The `is_empty` guard keeps a
/// blank comment (empty word list) from vacuously satisfying `all` and being
/// dropped as numeric.
fn is_all_numeric_label(words: &[String]) -> bool {
    !words.is_empty()
        && words.iter().all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()))
}

fn prefix_passes_entropy(prefix: &[String]) -> bool {
    let mut distinct: FxHashSet<&str> = FxHashSet::default();
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

    // The image-rs doc-comment from issue #7060, wide enough to clear `min_words`.
    // The trailing declaration name is filled per-file so the two copies land on
    // unrelated (non-parallel) declarations, the way a fuzz harness inlines a
    // renamed helper — otherwise the same-named cross-file exemption would apply.
    const RGBA_DOC: &str = "\
/// Write an Rgba32FImage assuming the writer is already buffered so in most
/// cases you should wrap your writer inside a BufWriter for the best throughput.\n";

    #[test]
    fn fuzz_harness_copy_not_flagged() {
        // Issue #7060: a fuzz harness deliberately copies the source helper and its
        // doc-comment (image-rs `fuzz/fuzzers/fuzzer_script_exr.rs`). The fuzz dir
        // is dropped from the corpus, so the src-side comment — whose only partner
        // is the fuzz copy — is not flagged.
        let dir = tempfile::tempdir().unwrap();
        let src = write(&dir, "src/codecs/openexr.rs", &format!("{RGBA_DOC}fn write_exr_frame() {{}}\n"));
        let fuzz = write(
            &dir,
            "fuzz/fuzzers/fuzzer_script_exr.rs",
            &format!("{RGBA_DOC}fn fuzz_harness_entry() {{}}\n"),
        );
        assert!(run(&[&src, &fuzz]).is_empty(), "fuzz-harness doc copies are exempt");
    }

    #[test]
    fn duplicate_doc_across_two_src_files_still_flagged() {
        // Control: the same copy between two real source files stays a smell. The
        // two functions carry unrelated names so no parallel-decl exemption applies.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "src/codecs/a.rs", &format!("{RGBA_DOC}fn write_exr_frame() {{}}\n"));
        let b = write(&dir, "src/codecs/b.rs", &format!("{RGBA_DOC}fn store_pixel_atlas() {{}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicate docs across real source files are still flagged");
        assert!(diags[0].message.contains("Near-duplicate comment"));
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

    /// The IDux MIT banner: a `/** */` block opening with the `@license` JSDoc tag
    /// and declaring an `MIT-style license` in prose — neither an SPDX id nor a
    /// `copyright`/`licensed under` phrase.
    const IDUX_MIT_HEADER: &str = "\
/**
 * @license
 *
 * Use of this source code is governed by an MIT-style license that can be
 * found in the LICENSE file at https://github.com/IDuxFE/idux/blob/main/LICENSE
 */
";

    #[test]
    fn ignores_jsdoc_license_banner() {
        // Regression (#5038): IDux ships this `@license` MIT banner atop all 1215
        // files. The `@license` tag and the `MIT-style license` prose are both
        // license-header markers, so the banner must be excluded across files.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "popper.ts", &format!("{IDUX_MIT_HEADER}export const a = 1;\n"));
        let b = write(&dir, "a11y.ts", &format!("{IDUX_MIT_HEADER}export const b = 2;\n"));
        assert!(run(&[&a, &b]).is_empty(), "@license / MIT-style banner must be excluded");
    }

    #[test]
    fn ignores_named_license_banner_without_jsdoc_tag() {
        // The prose license-name signature stands alone: a banner that names its
        // license (`BSD license`) in prose, with no `@license` tag or SPDX id,
        // is still a license header repeated per file.
        let dir = tempfile::tempdir().unwrap();
        let banner = "// This source code is released under the BSD license and may be freely redistributed and modified under those documented terms.\n";
        let a = write(&dir, "a.ts", &format!("{banner}export const a = 1;\n"));
        let b = write(&dir, "b.ts", &format!("{banner}export const b = 2;\n"));
        assert!(run(&[&a, &b]).is_empty(), "prose-named license banner must be excluded");
    }

    #[test]
    fn still_flags_duplicate_comment_merely_mentioning_a_license_word() {
        // Over-exclusion guard for #5038: a genuine explanatory comment that uses
        // the word `license` in prose — but names no license and carries no banner
        // marker — is ordinary duplicated prose and must still flag.
        let dir = tempfile::tempdir().unwrap();
        let note = "\
// The seat allocator checks the remaining license entitlement before granting a
// session, so an overcommitted tenant is refused instead of silently exceeding.
let x = 1;
";
        let a = write(&dir, "a.ts", note);
        let b = write(&dir, "b.ts", note);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "comment merely mentioning a license word still flags");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    /// The standard 16-line Apache 2.0 header. `licensed to the` opens line 1 and
    /// `copyright` sits on line 4 — mid-block, where it used to fragment the banner.
    const APACHE_HEADER: &str = "\
// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// \"License\"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// \"AS IS\" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.
";

    #[test]
    fn ignores_multiline_license_banner() {
        // Regression (#3907): the Apache header is one `//` run whose banner words
        // (`licensed to the` on line 1, `copyright` on line 4) used to fragment the
        // block — each fragment then lost its marker and was flagged. A banner word
        // on any line must now keep the run whole so the whole block is excluded.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "a.rs", &format!("{APACHE_HEADER}pub fn f() {{}}\n"));
        let b = write(&dir, "b.rs", &format!("{APACHE_HEADER}pub fn g() {{}}\n"));
        assert!(run(&[&a, &b]).is_empty(), "multi-line license banner must be excluded whole");
    }

    #[test]
    fn flags_duplicate_multiline_prose_block() {
        // Over-exclusion guard: a genuine multi-line prose docblock shared verbatim
        // across files — no banner, no directive — is real duplication and must
        // still flag, even though it is one merged `//` run like the banner.
        let dir = tempfile::tempdir().unwrap();
        let prose = "\
// The migration runner walks every pending entry in lexical order and applies
// them inside one transaction so a partial failure rolls back cleanly and the
// schema never lands in a half-migrated state across any deployment environment.
pub fn run() {}
";
        let a = write(&dir, "a.rs", prose);
        let b = write(&dir, "b.rs", prose);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicated multi-line prose is still a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
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
        // A doc-comment copy-pasted onto two *differently named*, unrelated
        // functions is genuine drift-prone duplication and must still flag.
        let dir = tempfile::tempdir().unwrap();
        let head = "/// Builds the canonical pagination defaults derived from the shared schema so every list stays consistent everywhere.\n";
        let a = write(&dir, "a.rs", &format!("{head}pub fn build_admin_list() {{}}\n"));
        let b = write(&dir, "b.rs", &format!("{head}pub fn build_lab_section() {{}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_parallel_implementation_doc_comments() {
        // Regression (#4842): two files are parallel backend implementations of
        // the same API (RustCrypto's 64-bit and 32-bit fixsliced AES), so each
        // exposes the same-named `aes128_decrypt` with the same doc-comment — it
        // describes the same item, not a copy-paste smell.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Fully-fixsliced AES-128 decryption (the InvShiftRows is completely omitted).
///
/// Decrypts four blocks in-place and in parallel.
pub(crate) fn aes128_decrypt(rkeys: u8, blocks: u8) -> u8 { rkeys ^ blocks }
";
        let a = write(&dir, "fixslice64.rs", doc);
        let b = write(&dir, "fixslice32.rs", doc);
        assert!(run(&[&a, &b]).is_empty(), "same-named parallel-impl docs must not flag");
    }

    #[test]
    fn ignores_parallel_builder_methods_with_variant_names() {
        // Regression (#5027): hyper's server and client HTTP/1 builders expose
        // the same parser knob under analogous-but-not-identical method names —
        // `ignore_invalid_headers` (server) and its `_in_responses` client twin.
        // The identical doc-comment describes the same behavior on both, so it is
        // intentional parallel documentation, not copy-paste drift.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Set whether HTTP/1 connections will silently ignore malformed header lines.
///
/// If this is enabled and a header line does not start with a valid header
/// name, or does not include a colon at all, the line will be silently ignored.
";
        let server = format!(
            "impl Builder {{\n{doc}    pub fn ignore_invalid_headers(&mut self, e: bool) -> &mut Self {{ self }}\n}}\n"
        );
        let client = format!(
            "impl Builder {{\n{doc}    pub fn ignore_invalid_headers_in_responses(&mut self, e: bool) -> &mut Self {{ self }}\n}}\n"
        );
        let a = write(&dir, "server.rs", &server);
        let b = write(&dir, "client.rs", &client);
        assert!(run(&[&a, &b]).is_empty(), "parallel builder variant docs must not flag");
    }

    #[test]
    fn ignores_parallel_safe_unsafe_decompressor_impl_docs() {
        // Regression (#5312): lz4_flex ships a safe (bounds-checked) and an unsafe
        // (unchecked-indexing) implementation of the same LZ4 decompressor in two
        // files. The two functions differ only by an implementation-direction
        // qualifier (`read_integer` safe / `read_integer_ptr` unsafe) and carry
        // the identical algorithm-description doc because they implement the same
        // protocol steps — intentional parallel documentation, not a copy-paste.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Read an integer.
///
/// In LZ4, we encode small integers in a way that we can have an arbitrary number
/// of bytes. In particular, we add the bytes repeatedly until we hit a non-0xFF
/// byte. When we do, we add this byte to our sum and terminate the loop.\n";
        let safe = write(
            &dir,
            "decompress_safe.rs",
            &format!("{doc}pub(super) fn read_integer(input: u8) -> u8 {{ input }}\n"),
        );
        let unsafe_ = write(
            &dir,
            "decompress.rs",
            &format!("{doc}pub(super) fn read_integer_ptr(input: u8) -> u8 {{ input }}\n"),
        );
        assert!(
            run(&[&safe, &unsafe_]).is_empty(),
            "safe/unsafe decompressor twins sharing algorithm docs must not flag"
        );
    }

    #[test]
    fn ignores_complete_mode_parser_twin_docs() {
        // Regression (#7071): nom ships a streaming `parse` alongside a
        // `parse_complete` (and `split_at_position` / `split_at_position_complete`,
        // plus the `1` arity variant), the `_complete` twin treating end-of-input
        // as success rather than `Incomplete`. Each twin opens its doc verbatim
        // like its base because it performs the same operation — twin-by-design
        // API docs, not copy-paste drift. `complete` is an impl-direction qualifier
        // so the co-located twins reduce to the same root and stay exempt.
        let dir = tempfile::tempdir().unwrap();
        let content = "\
pub trait Parser {
    /// A parser takes an input type and returns either the remaining input and the parsed output value or an error.
    fn parse(&mut self, input: u8) -> u8 { input }

    /// A parser takes an input type and returns either the remaining input and the parsed output value or an error.
    fn parse_complete(&mut self, input: u8) -> u8 { input }
}

pub trait Input {
    /// Looks for the first element of the input for which the predicate returns true and returns the input up to that position.
    fn split_at_position(&self, _p: u8) -> u8 { 0 }

    /// Looks for the first element of the input for which the predicate returns true and returns the input up to that position.
    fn split_at_position_complete(&self, _p: u8) -> u8 { 0 }

    /// Same as split_at_position but requires at least one matching element before the predicate becomes true anywhere here.
    fn split_at_position1(&self, _p: u8) -> u8 { 0 }

    /// Same as split_at_position but requires at least one matching element before the predicate becomes true anywhere here.
    fn split_at_position1_complete(&self, _p: u8) -> u8 { 0 }
}
";
        let a = write(&dir, "nom_traits.rs", content);
        // A second file keeps the run active (a single-file run is a noop).
        let filler = write(&dir, "filler.rs", "pub fn z() {}\n");
        assert!(
            run(&[&a, &filler]).is_empty(),
            "streaming/complete-mode parser twin docs in one file must not flag"
        );
    }

    /// `documented_decl_name` of the first comment node in a parsed TypeScript
    /// snippet — the direct seam the directive-comment skip lives on.
    fn first_comment_decl_name(source: &str) -> Option<String> {
        let mut parser = Parser::new();
        let tree = parse_with_grammar(&mut parser, Language::TypeScript, source.as_bytes())
            .expect("typescript grammar parses");
        let root = tree.root_node();
        let mut cursor = root.walk();
        let comment = root
            .children(&mut cursor)
            .find(|n| is_comment_kind(n.kind()))
            .expect("snippet has a leading comment");
        documented_decl_name(comment, source.as_bytes()).name
    }

    #[test]
    fn attributes_jsdoc_to_function_through_intervening_directive() {
        // Regression (#7045): a `// @ts-expect-error` directive between a JSDoc
        // block and its `function` used to leave the block's next named sibling
        // pointing at the directive comment, dropping the attribution. The doc now
        // resolves through the directive to the declaration it documents.
        let name = first_comment_decl_name(
            "/**\n * Caches the resolved schema output.\n */\n// @ts-expect-error\nfunction cacheAsync() {}\n",
        );
        assert_eq!(name.as_deref(), Some("cacheAsync"));
    }

    #[test]
    fn attributes_jsdoc_directly_above_function_unchanged() {
        // Control: with no intervening directive the attribution is unchanged.
        let name = first_comment_decl_name(
            "/**\n * Caches the resolved schema output.\n */\nfunction cacheAsync() {}\n",
        );
        assert_eq!(name.as_deref(), Some("cacheAsync"));
    }

    #[test]
    fn attributes_jsdoc_through_multiple_intervening_directives() {
        // Several stacked directive lines between the doc block and the function
        // are all skipped to reach the declaration.
        let name = first_comment_decl_name(
            "/**\n * Caches the resolved schema output.\n */\n// @ts-expect-error\n// eslint-disable-next-line\nfunction cacheAsync() {}\n",
        );
        assert_eq!(name.as_deref(), Some("cacheAsync"));
    }

    #[test]
    fn does_not_skip_past_a_following_doc_block() {
        // The skip stops at a following *doc*-comment block: the nearer block, not
        // the one above it, documents the declaration. The earlier block stays
        // unattributed so a stray copy-pasted lead block is never exempted.
        let name = first_comment_decl_name(
            "/**\n * Some earlier unrelated note block here.\n */\n/**\n * Caches the resolved schema output.\n */\nfunction cacheAsync() {}\n",
        );
        assert_eq!(name, None);
    }

    #[test]
    fn ignores_parallel_api_doc_with_intervening_directive() {
        // End-to-end (#7045): parallel-API twin functions across two files share
        // the same JSDoc by design. A `// @ts-expect-error` directive sits between
        // the JSDoc and the function in one file; the directive skip lets the
        // same-name parallel-API exemption still fire, so the doc is not flagged.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/**
 * Caches the fully-resolved output of the provided schema so repeated
 * validations reuse the memoized dataset instead of recomputing everything.
 */
";
        let sync = write(&dir, "cache.ts", &format!("{doc}function memoize() {{}}\n"));
        let with_directive = write(
            &dir,
            "cacheAsync.ts",
            &format!("{doc}// @ts-expect-error\nfunction memoize() {{}}\n"),
        );
        assert!(
            run(&[&sync, &with_directive]).is_empty(),
            "parallel-API doc must not flag when a directive separates JSDoc from its function"
        );
    }

    #[test]
    fn still_flags_duplicate_doc_with_directive_on_unrelated_decls() {
        // Over-exclusion guard for #7045: the directive skip must not suppress a
        // genuine duplicate. The same JSDoc copy-pasted onto two *differently
        // named*, unrelated functions across files is real duplication and still
        // flags, even with an intervening directive.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/**
 * Caches the fully-resolved output of the provided schema so repeated
 * validations reuse the memoized dataset instead of recomputing everything.
 */
";
        let a = write(&dir, "a.ts", &format!("{doc}function buildAdminList() {{}}\n"));
        let b = write(
            &dir,
            "b.ts",
            &format!("{doc}// @ts-expect-error\nfunction buildLabSection() {{}}\n"),
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicate doc on unrelated decls still flags through a directive");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_duplicate_doc_on_non_qualifier_suffixed_decls() {
        // The twin exemption only strips a recognized implementation-direction
        // qualifier (`_ptr`, `_safe`, `_simd`, …). Two functions whose names
        // differ by an ordinary suffix (`_db`) are unrelated, so an identical doc
        // copy-pasted onto both is real duplication and must still flag.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Builds the canonical pagination defaults derived from the shared schema so
/// every list view stays consistent across the whole admin surface everywhere.\n";
        let a = write(&dir, "a.rs", &format!("{doc}pub fn init() {{}}\n"));
        let b = write(&dir, "b.rs", &format!("{doc}pub fn init_db() {{}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a non-qualifier suffix is not a parallel-impl signal");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn fallible_try_prefix_pairs_with_infallible_twin() {
        // `try_X` is the canonical fallible variant of the infallible `X`; the
        // relationship is a whole-name `try_` prefix, so it holds even when the
        // shared root itself ends in a qualifier segment (`copy_to_slice`).
        assert!(are_impl_qualifier_twins("get_u8", "try_get_u8"));
        assert!(are_impl_qualifier_twins("try_get_u8", "get_u8"));
        assert!(are_impl_qualifier_twins("copy_to_slice", "try_copy_to_slice"));
        // The `_` boundary keeps a bare `tryout` from collapsing onto `out`, and
        // two genuinely unrelated names are never twins.
        assert!(!are_impl_qualifier_twins("out", "tryout"));
        assert!(!are_impl_qualifier_twins("get_u8", "put_u8"));
    }

    #[test]
    fn camel_case_async_sync_suffix_pairs_are_twins() {
        // The JS/TS sync/async convention: one base plus a camelCase `Async`/`Sync`
        // suffix. `parse` / `parseAsync`, either order, and the `safeParse` twin.
        assert!(are_async_sync_twins("parse", "parseAsync"));
        assert!(are_async_sync_twins("parseAsync", "parse"));
        assert!(are_async_sync_twins("safeParse", "safeParseAsync"));
        // Both variants suffixed: `parseSync` / `parseAsync` share the `parse` base.
        assert!(are_async_sync_twins("parseSync", "parseAsync"));
        // The uppercase `A`/`S` is the word boundary: a lowercase `async`/`sync`
        // ending (`resync`) is not a suffix, so it never pairs with a bare prefix.
        assert!(!are_async_sync_twins("resync", "re"));
        assert!(!are_async_sync_twins("myasync", "my"));
        // A bare suffix leaves an empty prefix, so it is left intact and never
        // collapses two unrelated names onto one empty root.
        assert!(!are_async_sync_twins("Async", "Sync"));
        // Different bases after stripping stay distinct (`render` vs `parse`).
        assert!(!are_async_sync_twins("render", "parseAsync"));
    }

    #[test]
    fn endianness_be_le_prefix_pairs_are_impl_qualifier_twins() {
        // `be_`/`le_` are big-/little-endian direction qualifiers: for a 1-byte
        // integer both directions read identically, so the pair reduces to one
        // root and shares a doc by design (nom's `be_u8` / `le_u8`).
        assert!(are_impl_qualifier_twins("be_u8", "le_u8"));
        assert!(are_impl_qualifier_twins("be_i8", "le_i8"));
        // Same endianness, different width is not a twin — the roots differ.
        assert!(!are_impl_qualifier_twins("be_u8", "be_i8"));
        // The qualifier only strips a full `_`-delimited leading segment, so a
        // single-segment name that merely starts with the letters be/le never
        // collapses onto another one.
        assert!(!are_impl_qualifier_twins("beacon", "ledger"));
        assert!(!are_impl_qualifier_twins("alpha", "beta"));
    }

    #[test]
    fn ignores_try_fallible_variant_twin_docs() {
        // Regression (#6263): tokio-rs/bytes ships ~27 infallible/fallible method
        // twins in one trait (`get_u8` / `try_get_u8`, `copy_to_slice` /
        // `try_copy_to_slice`, …). The `try_` variant opens its doc verbatim like
        // the infallible one and only swaps the `# Panics` clause for a
        // `Returns Err(…)` clause — twin-by-design API documentation, not
        // copy-paste drift. Both methods live in the same file, so the exemption
        // must hold for co-located impl-qualifier twins, including a root that
        // ends in a qualifier segment (`copy_to_slice`).
        let dir = tempfile::tempdir().unwrap();
        let content = "\
pub trait Buf {
    /// Gets an unsigned 8 bit integer value from the current cursor inside `self`.
    ///
    /// The internal read position is then advanced by exactly one single byte.
    ///
    /// # Panics
    ///
    /// This call panics whenever there is not enough remaining data left to read.
    fn get_u8(&mut self) -> u8 {
        0
    }

    /// Gets an unsigned 8 bit integer value from the current cursor inside `self`.
    ///
    /// The internal read position is then advanced by exactly one single byte.
    ///
    /// Returns `Err(TryGetError)` whenever there is not enough remaining data left.
    fn try_get_u8(&mut self) -> Result<u8, ()> {
        Ok(0)
    }

    /// Copies bytes from `self` into the provided destination slice given by caller.
    ///
    /// The cursor is advanced by the total number of bytes that were copied over.
    ///
    /// # Panics
    ///
    /// This call panics when `self` does not contain enough bytes for the slice.
    fn copy_to_slice(&mut self, _dst: &mut [u8]) {}

    /// Copies bytes from `self` into the provided destination slice given by caller.
    ///
    /// The cursor is advanced by the total number of bytes that were copied over.
    ///
    /// Returns `Err(TryGetError)` when `self` does not contain enough remaining bytes.
    fn try_copy_to_slice(&mut self, _dst: &mut [u8]) -> Result<(), ()> {
        Ok(())
    }
}
";
        let a = write(&dir, "buf_impl.rs", content);
        // A second file keeps the run active (a single-file run is a noop).
        let filler = write(&dir, "filler.rs", "pub fn z() {}\n");
        assert!(
            run(&[&a, &filler]).is_empty(),
            "try_/non-try_ infallible-fallible twin docs in one file must not flag"
        );
    }

    #[test]
    fn still_flags_intra_file_duplicate_doc_on_non_twin_decls() {
        // The co-located twin exemption stays surgical: two functions in one file
        // whose names are not impl-qualifier twins (`encode` / `decode`) carrying
        // a verbatim copy-pasted doc are real intra-file duplication and flag.
        let dir = tempfile::tempdir().unwrap();
        let content = "\
/// Builds the canonical pagination defaults derived from the shared schema so
/// every list view stays consistent across the whole admin surface everywhere.
pub fn encode() {}

/// Builds the canonical pagination defaults derived from the shared schema so
/// every list view stays consistent across the whole admin surface everywhere.
pub fn decode() {}
";
        let a = write(&dir, "codec.rs", content);
        // A second file keeps the run active (a single-file run is a noop).
        let filler = write(&dir, "filler.rs", "pub fn z() {}\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "intra-file copy-paste on non-twin decls is real duplication");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_be_le_endianness_twin_docs() {
        // Regression (#7070): nom ships `be_u8`/`le_u8` and `be_i8`/`le_i8` byte
        // parsers side by side in one module. For a 1-byte integer big- and
        // little-endian read identically, so each `le_` variant opens its doc
        // verbatim like its `be_` twin — parallel endianness-direction API
        // documentation, not copy-paste drift. The two width pairs carry distinct
        // docs so each stays paired with its own endianness twin.
        let dir = tempfile::tempdir().unwrap();
        let unsigned = "\
/// Recognizes an unsigned 1 byte integer parsed directly from the input stream returning the parsed value.\n";
        let signed = "\
/// Recognizes a signed 1 byte integer decoded straight out of the provided buffer returning the decoded value.\n";
        let content = format!(
            "{unsigned}pub fn be_u8(i: u8) -> u8 {{ i }}\n\n{unsigned}pub fn le_u8(i: u8) -> u8 {{ i }}\n\n\
             {signed}pub fn be_i8(i: i8) -> i8 {{ i }}\n\n{signed}pub fn le_i8(i: i8) -> i8 {{ i }}\n"
        );
        let a = write(&dir, "number.rs", &content);
        // A second file keeps the run active (a single-file run is a noop).
        let filler = write(&dir, "filler.rs", "pub fn z() {}\n");
        assert!(
            run(&[&a, &filler]).is_empty(),
            "be_/le_ endianness twins sharing byte-parser docs must not flag"
        );
    }

    #[test]
    fn still_flags_be_le_prefixed_single_segment_non_twins() {
        // The endianness exemption strips `be`/`le` only as a full `_`-delimited
        // segment, so single-segment names that merely start with those letters
        // (`beacon` / `ledger`) are not twins and a copy-pasted doc on both is
        // real duplication that must still flag.
        let dir = tempfile::tempdir().unwrap();
        let content = "\
/// Builds the canonical pagination defaults derived from the shared schema so
/// every list view stays consistent across the whole admin surface everywhere.
pub fn beacon() {}

/// Builds the canonical pagination defaults derived from the shared schema so
/// every list view stays consistent across the whole admin surface everywhere.
pub fn ledger() {}
";
        let a = write(&dir, "misc.rs", content);
        // A second file keeps the run active (a single-file run is a noop).
        let filler = write(&dir, "filler.rs", "pub fn z() {}\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "be/le are not qualifiers unless a full leading segment");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_duplicate_non_doc_comment_without_named_decl() {
        // Over-exclusion guard for #5027: a plain `//` rationale copy-pasted
        // across files with no distinct named declaration below it is real
        // copy-paste cruft and must still flag — the variant-name exemption only
        // covers doc-comments on parallel named declarations.
        let dir = tempfile::tempdir().unwrap();
        let note = "\
// Our defaults are chosen for the majority case, which usually are not resource
// constrained, and so the spec default of sixty-four kilobytes can be too small.
let x = 1;
";
        let a = write(&dir, "server.rs", note);
        let b = write(&dir, "client.rs", note);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicated free-floating note is still a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_duplicate_doc_on_unrelated_short_named_decls() {
        // The variant exemption must stay surgical: two functions whose names
        // share only a short generic root (`init` ⊂ `init_db`) carrying the same
        // doc-comment are unrelated copy-paste, not a parallel API pair.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Builds the canonical pagination defaults derived from the shared schema so
/// every list view stays consistent across the whole admin surface everywhere.
";
        let a = write(&dir, "a.rs", &format!("{doc}pub fn init() {{}}\n"));
        let b = write(&dir, "b.rs", &format!("{doc}pub fn init_db() {{}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "short shared root is not a parallel-API signal");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_duplicate_doc_on_differently_named_decls() {
        // The exemption is name-keyed: an identical doc-comment over two
        // *differently named* functions across files is real duplication. Mirrors
        // the parallel-impl shape but with distinct names so it must still flag.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Fully-fixsliced AES-128 decryption (the InvShiftRows is completely omitted).
///
/// Decrypts four blocks in-place and in parallel.
";
        let a = write(&dir, "a.rs", &format!("{doc}pub fn decode_alpha(x: u8) -> u8 {{ x }}\n"));
        let b = write(&dir, "b.rs", &format!("{doc}pub fn decode_beta(x: u8) -> u8 {{ x }}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "identical doc on differently-named decls is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_camel_case_async_sync_twin_jsdoc() {
        // Regression (#7044): valibot ships sync/async twin functions (`parse` /
        // `parseAsync`, `safeParse` / `safeParseAsync`) in sibling files, each
        // carrying identical JSDoc because it describes the same operation — the
        // JS/TS counterpart of Rust's `_async`/`_sync` twins, not a copy-paste.
        let dir = tempfile::tempdir().unwrap();
        let parse_doc = "\
/**
 * Parses an unknown input based on the schema and returns the fully typed parsed output value.
 */\n";
        let safe_doc = "\
/**
 * Safely parses an unknown input based on the schema and returns a typed result without throwing here.
 */\n";
        let parse = write(&dir, "parse.ts", &format!("{parse_doc}export function parse() {{}}\n"));
        let parse_async = write(
            &dir,
            "parseAsync.ts",
            &format!("{parse_doc}export async function parseAsync() {{}}\n"),
        );
        let safe = write(&dir, "safeParse.ts", &format!("{safe_doc}export function safeParse() {{}}\n"));
        let safe_async = write(
            &dir,
            "safeParseAsync.ts",
            &format!("{safe_doc}export async function safeParseAsync() {{}}\n"),
        );
        assert!(
            run(&[&parse, &parse_async, &safe, &safe_async]).is_empty(),
            "camelCase sync/async twin functions sharing JSDoc must not flag"
        );
    }

    #[test]
    fn still_flags_duplicate_doc_on_non_async_sync_twin_decls() {
        // The camelCase twin exemption stays surgical: two functions whose bases
        // differ after stripping an `Async`/`Sync` suffix (`render` vs `parseAsync`),
        // or whose name merely ends in lowercase `sync` with no word boundary
        // (`resync` vs `re`), are unrelated, so identical docs are real duplication.
        let dir = tempfile::tempdir().unwrap();
        let render_doc = "\
/**
 * Renders an unknown input based on the schema and returns the fully typed rendered output here now.
 */\n";
        let sync_doc = "\
/**
 * Rebuilds the shared pagination defaults derived from the canonical schema so every listing view stays consistent.
 */\n";
        let render = write(&dir, "render.ts", &format!("{render_doc}export function render() {{}}\n"));
        let parse_async = write(
            &dir,
            "parse-async.ts",
            &format!("{render_doc}export async function parseAsync() {{}}\n"),
        );
        let resync = write(&dir, "resync.ts", &format!("{sync_doc}export function resync() {{}}\n"));
        let re = write(&dir, "re.ts", &format!("{sync_doc}export function re() {{}}\n"));
        let diags = run(&[&render, &parse_async, &resync, &re]);
        assert_eq!(diags.len(), 2, "non-twin decls sharing identical docs are real duplication");
        assert!(diags.iter().all(|d| d.message.contains("Near-duplicate comment")));
    }

    #[test]
    fn ignores_mirror_prop_doc_between_js_props_object_and_ts_type() {
        // Regression (#4990): a Vue component ships a JS runtime props object and a
        // TS type for the same prop API, so the same prop's `//` description appears
        // above the runtime `pair` and above the matching `property_signature`. The
        // mirrored docs document the same named prop, not a copy-paste smell. The
        // comments are plain `//` (the per-member convention), so only the
        // same-named-member exemption keeps them out.
        let dir = tempfile::tempdir().unwrap();
        let js = "\
export const props = {
  // The array of events to display in Vue Cal. Can hold just the view events and
  // be updated, or the full array of all events available to the calendar here.
  events: { type: Array, default: () => [] },
};
";
        let ts = "\
export type VueCalProps = {
  // The array of events to display in Vue Cal. Can hold just the view events and
  // be updated, or the full array of all events available to the calendar here.
  events?: VueCalEvent[],
};
";
        let a = write(&dir, "props-definitions.js", js);
        let b = write(&dir, "vue-cal.ts", ts);
        assert!(
            run(&[&a, &b]).is_empty(),
            "mirror prop docs across a runtime props object and a TS type must not flag"
        );
    }

    #[test]
    fn still_flags_duplicate_prop_doc_on_differently_named_members() {
        // The member exemption is name-keyed too: the same `//` description copied
        // above two *differently named* props across files is real duplication.
        let dir = tempfile::tempdir().unwrap();
        let head = "\
  // The array of events to display in Vue Cal. Can hold just the view events and
  // be updated, or the full array of all events available to the calendar here.\n";
        let a = write(&dir, "a.ts", &format!("export type A = {{\n{head}  events?: number[],\n}};\n"));
        let b = write(&dir, "b.ts", &format!("export type B = {{\n{head}  sessions?: number[],\n}};\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "identical doc on differently-named props is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_same_field_jsdoc_across_request_and_response_interfaces() {
        // Regression (#5503): an OpenAPI-generated SDK emits each API field's one
        // canonical description as JSDoc on every type that carries the field — a
        // response interface AND its request `CreateParams` twin. The same
        // `minimum_amount` field documented identically on two *different*
        // interfaces in one file is intentional parallel API documentation, not a
        // copy-paste smell. Distinct interface owners keep the same-file member
        // exemption surgical: a field name is unique within its interface.
        let dir = tempfile::tempdir().unwrap();
        let jsdoc = "\
  /**
   * Minimum amount required to redeem this Promotion Code into a Coupon (e.g., a
   * purchase must be $100 or more to work) before the redemption is allowed here.
   */\n";
        let content = format!(
            "export interface Restrictions {{\n{jsdoc}  minimum_amount: number | null;\n}}\n\
             export interface RestrictionsCreateParams {{\n{jsdoc}  minimum_amount?: number;\n}}\n"
        );
        let a = write(&dir, "PromotionCodes.ts", &content);
        let b = write(&dir, "filler.ts", "export const x = 1;\n");
        assert!(
            run(&[&a, &b]).is_empty(),
            "same field's JSDoc on a request and a response interface must not flag"
        );
    }

    #[test]
    fn still_flags_intra_interface_copy_pasted_field_jsdoc() {
        // Over-exclusion guard for #5503: two *different* fields of the *same*
        // interface carrying the same copy-pasted JSDoc is real duplication. The
        // shared owner keeps the parallel-member exemption from firing, so a
        // botched copy-paste within one type still flags.
        let dir = tempfile::tempdir().unwrap();
        let jsdoc = "\
  /**
   * Minimum amount required to redeem this Promotion Code into a Coupon (e.g., a
   * purchase must be $100 or more to work) before the redemption is allowed here.
   */\n";
        let content = format!(
            "export interface Restrictions {{\n{jsdoc}  minimum_amount?: number;\n{jsdoc}  maximum_amount?: number;\n}}\n"
        );
        let a = write(&dir, "PromotionCodes.ts", &content);
        let b = write(&dir, "filler.ts", "export const x = 1;\n");
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "copy-pasted JSDoc on two fields of one interface is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_parallel_struct_field_comments_in_distinct_owners_issue_6272() {
        // Regression (#6272): http's `IterMut` and `ValueIterMut` are two distinct
        // iterator types that both hold a raw pointer to the same `extra_values`
        // backing store, so each documents the same-named field with the same
        // aliasing-invariant `//` comment. The identical doc describes the same
        // field on parallel owners, not a copy-paste smell. Distinct struct owners
        // keep the same-file member exemption surgical — a field name is unique per
        // struct.
        let dir = tempfile::tempdir().unwrap();
        let field_doc = "    // This raw pointer aliases the original extra values backing allocation for the entire lifetime of the iterator here.\n";
        let content = format!(
            "pub struct IterMut {{\n{field_doc}    extra_values: usize,\n}}\n\
             pub struct ValueIterMut {{\n{field_doc}    extra_values: usize,\n}}\n"
        );
        let a = write(&dir, "map.rs", &content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(
            run(&[&a, &b]).is_empty(),
            "same-named field's invariant comment on distinct structs must not flag"
        );
    }

    #[test]
    fn still_flags_intra_struct_copy_pasted_field_comment_issue_6272() {
        // Over-exclusion guard for #6272: two *different* fields of the *same*
        // struct carrying the same copy-pasted `//` comment is real duplication.
        // The shared owner keeps the parallel-member exemption from firing.
        let dir = tempfile::tempdir().unwrap();
        let field_doc = "    // This raw pointer aliases the original extra values backing allocation for the entire lifetime of the iterator here.\n";
        let content = format!(
            "pub struct IterMut {{\n{field_doc}    extra_values: usize,\n{field_doc}    other_values: usize,\n}}\n"
        );
        let a = write(&dir, "map.rs", &content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "copy-pasted comment on two fields of one struct is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_duplicate_comment_on_differently_named_struct_fields_issue_6272() {
        // The struct-field exemption is name-keyed: the same `//` comment over two
        // *differently named* fields of two different structs is real duplication.
        // `are_parallel_owned_members` keys on a matching member name, so distinct
        // owners alone do not exempt non-parallel field names.
        let dir = tempfile::tempdir().unwrap();
        let field_doc = "    // This raw pointer aliases the original extra values backing allocation for the entire lifetime of the iterator here.\n";
        let content = format!(
            "pub struct IterMut {{\n{field_doc}    extra_values: usize,\n}}\n\
             pub struct ValueIterMut {{\n{field_doc}    cached_length: usize,\n}}\n"
        );
        let a = write(&dir, "map.rs", &content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "identical comment on differently-named fields is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_parallel_implementation_ts_jsdoc() {
        // The exemption is cross-language: two TS files exposing the same-named
        // exported function with the same `/** */` JSDoc are parallel API
        // implementations, not a copy-paste smell.
        let dir = tempfile::tempdir().unwrap();
        let doc = |name: &str| {
            format!(
                "/**\n                 * Encrypts four blocks in-place and in parallel using the fixsliced\n                 * representation of the expanded round keys handed to it.\n                 */\n                 export function {name}(rkeys: number, blocks: number): number {{ return rkeys ^ blocks; }}\n"
            )
        };
        let a = write(&dir, "backend-wasm.ts", &doc("aes128Encrypt"));
        let b = write(&dir, "backend-node.ts", &doc("aes128Encrypt"));
        assert!(run(&[&a, &b]).is_empty(), "same-named TS parallel-impl JSDoc must not flag");
        // The name is what earns the exemption, and the block is long enough to
        // reach it: the same JSDoc over an unrelated export still flags.
        let c = write(&dir, "backend-deno.ts", &doc("hashPassword"));
        assert_eq!(run(&[&a, &c]).len(), 1, "an unrelated export still flags");
    }

    #[test]
    fn ignores_repeated_doc_on_one_declaration_name_in_one_scope() {
        // One name repeated in one scope is one item declared several times — the
        // shape TypeScript overload signatures take, and the shape Rust's own
        // name resolution forbids outright. Each such declaration needs its own
        // doc block, so the repetition is not a copy-paste. Distinct files filler
        // keeps the run from being a single-file noop.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Fully-fixsliced AES-128 decryption (the InvShiftRows is completely omitted).
///
/// Decrypts four blocks in-place and in parallel.
pub fn aes128_decrypt(x: u8) -> u8 { x }
";
        let a = write(&dir, "a.rs", &format!("{doc}\n{doc}"));
        let b = write(&dir, "filler.rs", "pub fn z() {}\n");
        assert!(run(&[&a, &b]).is_empty(), "one name in one scope is one documented item");
    }

    #[test]
    fn ignores_mirror_trait_method_docs_across_sync_async_crates() {
        // Regression (#4726): `embedded-hal` and `embedded-hal-async` mirror the
        // same trait, so each carries the same-named trait method (a
        // `function_signature_item`, not a `function_item`) with an identical doc
        // describing the same API contract — intentional API mirroring, not a copy.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
pub trait SpiDevice {
    /// Perform a transaction against the device, locking the bus for its whole
    /// duration so no other transaction can run concurrently against the same bus.
    ///
    /// The locking mechanism is implementation-defined and must keep two such
    /// transactions from ever executing concurrently against one shared bus here.
    fn transaction(&mut self, operations: u8) -> u8;
}
";
        let a = write(&dir, "spi.rs", doc);
        let b = write(&dir, "async_spi.rs", doc);
        assert!(
            run(&[&a, &b]).is_empty(),
            "mirrored trait-method docs across parallel crates must not flag"
        );
    }

    #[test]
    fn ignores_mirror_associated_type_docs_across_crates() {
        // An associated type in mirrored sync/async traits carries the same doc.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
pub trait ErrorType {
    /// Error type used by this trait, threaded through every operation so callers
    /// can recover from a transport failure without losing the in-flight context.
    type Error;
}
";
        let a = write(&dir, "io.rs", doc);
        let b = write(&dir, "async_io.rs", doc);
        assert!(
            run(&[&a, &b]).is_empty(),
            "mirrored associated-type docs across parallel crates must not flag"
        );
    }

    #[test]
    fn still_flags_duplicate_doc_on_differently_named_trait_methods() {
        // The exemption stays name-keyed for trait methods too: the same doc
        // copy-pasted onto two *differently named* trait methods across files is
        // real duplication and must still flag.
        let dir = tempfile::tempdir().unwrap();
        let head = "\
    /// Perform a transaction against the device, locking the bus for its whole
    /// duration so no other transaction can run concurrently against the same bus.
    ///
    /// The locking mechanism is implementation-defined and must keep two such
    /// transactions from ever executing concurrently against one shared bus here.\n";
        let a = write(&dir, "a.rs", &format!("pub trait A {{\n{head}    fn read(&mut self) -> u8;\n}}\n"));
        let b = write(&dir, "b.rs", &format!("pub trait B {{\n{head}    fn write(&mut self) -> u8;\n}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "identical doc on differently-named trait methods is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_same_named_associated_type_docs_across_traits_issue_7073() {
        // Regression (#7073): nom's `Input::Item` and `ExtendInto::Item` are the
        // same-named associated type in two *different* traits, each carrying the
        // same doc because it describes the same concept (the element type of a
        // sequence). A same member name under distinct trait owners is a parallel
        // API surface, mirroring the struct-field and interface-member exemptions,
        // not a copy-paste smell.
        let dir = tempfile::tempdir().unwrap();
        let doc = "    /// The current input type is a sequence of that item type, so every element the parser consumes is one of these.\n";
        let content = format!(
            "pub trait Input {{\n{doc}    type Item;\n}}\n\
             pub trait ExtendInto {{\n{doc}    type Item;\n}}\n"
        );
        let a = write(&dir, "traits.rs", &content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(
            run(&[&a, &b]).is_empty(),
            "same-named associated type's doc on distinct traits must not flag"
        );
    }

    #[test]
    fn ignores_same_named_trait_method_docs_across_traits_in_one_file() {
        // The same owner-tracking fix covers trait methods: a same-named
        // `function_signature_item` declared in two *different* traits in one file,
        // each documented identically, is a parallel trait-API surface once the
        // method inherits its enclosing trait as owner.
        let dir = tempfile::tempdir().unwrap();
        let doc = "    /// Process the pending record and flush every derived side effect before the next batch of work begins here.\n";
        let content = format!(
            "pub trait Reader {{\n{doc}    fn process(&self);\n}}\n\
             pub trait Writer {{\n{doc}    fn process(&self);\n}}\n"
        );
        let a = write(&dir, "io.rs", &content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(
            run(&[&a, &b]).is_empty(),
            "same-named trait method's doc on distinct traits must not flag"
        );
    }

    #[test]
    fn still_flags_intra_trait_copy_pasted_associated_type_docs_issue_7073() {
        // Over-exclusion guard for #7073: two *differently named* associated types
        // of the *same* trait carrying the same copy-pasted doc is real duplication.
        // The shared trait owner keeps the parallel-member exemption from firing.
        let dir = tempfile::tempdir().unwrap();
        let doc = "    /// The current input type is a sequence of that item type, so every element the parser consumes is one of these.\n";
        let content =
            format!("pub trait Input {{\n{doc}    type Item;\n{doc}    type Chunk;\n}}\n");
        let a = write(&dir, "traits.rs", &content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "copy-pasted doc on two associated types of one trait is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn impl_associated_type_docs_stay_owner_less_no_wrong_attribution() {
        // Guard for the added `declaration_list` transit: it is also an `impl` /
        // `mod` body, but neither is a named owner. Two impls of *different* types
        // declaring a same-named associated type with an identical doc must still
        // flag — the impl members resolve to no owner, so the parallel-member
        // exemption cannot misattribute them to the impl target and wrongly suppress
        // a genuine duplicate.
        let dir = tempfile::tempdir().unwrap();
        let doc = "    /// The concrete element type this adapter yields on every step of the walk is fixed once here for callers.\n";
        let content = format!(
            "impl Iterator for Foo {{\n{doc}    type Item = u8;\n}}\n\
             impl Iterator for Bar {{\n{doc}    type Item = u8;\n}}\n"
        );
        let a = write(&dir, "impls.rs", &content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicate doc on impl associated types has no named owner and still flags");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    // typeorm's `QueryRunner.clearDatabase` contract, carried verbatim by the
    // interface and by every class that implements it.
    const CLEAR_DATABASE_DOC: &str = "\
/**
 * Removes all tables from the currently connected database.
 * Be careful with using this method and avoid using it in production or
 * migrations (because it can clear all your database).
 */
";

    #[test]
    fn ignores_interface_method_jsdoc_mirrored_by_implementing_class_issue_6870() {
        // Regression (#6870): typeorm states the `clearDatabase` contract on the
        // `QueryRunner` interface and again on each implementation. The interface
        // spells the method as a `method_signature`, the class as a
        // `method_definition`.
        let dir = tempfile::tempdir().unwrap();
        let interface_file = write(
            &dir,
            "QueryRunner.ts",
            &format!(
                "export interface QueryRunner {{\n{CLEAR_DATABASE_DOC}\
                 clearDatabase(database?: string): Promise<void>\n}}\n"
            ),
        );
        let class_file = write(
            &dir,
            "CordovaQueryRunner.ts",
            &format!(
                "export class CordovaQueryRunner implements QueryRunner {{\n{CLEAR_DATABASE_DOC}\
                 async clearDatabase(): Promise<void> {{}}\n}}\n"
            ),
        );
        assert!(
            run(&[&interface_file, &class_file]).is_empty(),
            "an interface method's JSDoc mirrored by its implementation must not flag"
        );
    }

    #[test]
    fn ignores_abstract_method_jsdoc_mirrored_by_concrete_subclass() {
        // The `abstract_method_signature` declares the method and the subclass
        // `method_definition` implements it.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/**
 * Flushes every buffered migration statement to the transport and waits until
 * the remote acknowledges the batch before the next scheduled flush starts.
 */
";
        let base = write(
            &dir,
            "BaseLogger.ts",
            &format!(
                "export abstract class BaseLogger {{\n{doc}\
                 abstract flush(): Promise<void>\n}}\n"
            ),
        );
        let concrete = write(
            &dir,
            "FileLogger.ts",
            &format!(
                "export class FileLogger extends BaseLogger {{\n{doc}\
                 async flush(): Promise<void> {{}}\n}}\n"
            ),
        );
        assert!(
            run(&[&base, &concrete]).is_empty(),
            "an abstract method's JSDoc mirrored by its implementation must not flag"
        );
    }

    #[test]
    fn ignores_overload_signature_jsdoc_mirrored_across_platform_modules() {
        // A module-level overload signature is a `function_signature`. TypeScript
        // has no syntax for sharing one JSDoc block across overloads — editors
        // resolve the doc per selected signature — so the block is repeated per
        // signature and per platform build of the same export.
        let dir = tempfile::tempdir().unwrap();
        let head = "\
/**
 * Parses the migration manifest into every pending revision in the exact order
 * the runner must apply them to reach the requested target schema version.
 */
export function parseManifest(raw: string): string[];
export function parseManifest(raw: string, strict?: boolean): string[] {
";
        let node = write(
            &dir,
            "manifest.node.ts",
            &format!("{head}    return readFileSync(raw, 'utf8').split(EOL)\n}}\n"),
        );
        let browser =
            write(&dir, "manifest.browser.ts", &format!("{head}    return raw.split('\\n')\n}}\n"));
        assert!(
            run(&[&node, &browser]).is_empty(),
            "an overload signature's JSDoc mirrored by the other platform module must not flag"
        );
    }

    #[test]
    fn ignores_same_named_method_docs_across_interfaces_in_one_file() {
        // An interface method inherits its interface as owner, so the owned-member
        // exemption reaches it. That exemption is file-agnostic on purpose: a member
        // name is unique inside its owner, so two matching docs can only come from
        // two different interfaces — the reasoning that already exempts a same-named
        // `property_signature` under two interfaces and a same-named trait method
        // under two traits.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "export interface QueryRunner {{\n{CLEAR_DATABASE_DOC}\
             clearDatabase(database?: string): Promise<void>\n}}\n\
             export interface SchemaBuilder {{\n{CLEAR_DATABASE_DOC}\
             clearDatabase(database?: string): Promise<void>\n}}\n"
        );
        let a = write(&dir, "contracts.ts", &content);
        let b = write(&dir, "filler.ts", "export const x = 1;\n");
        assert!(
            run(&[&a, &b]).is_empty(),
            "same-named method's JSDoc on distinct interfaces must not flag"
        );
    }

    #[test]
    fn still_flags_same_named_abstract_method_docs_across_classes_in_one_file() {
        // Boundary of the owner walk: `enclosing_named_type` transits an
        // `interface_body` but not a `class_body`, so a method declared in a class
        // inherits no owner and the owned-member exemption cannot reach it. Two
        // classes in one file therefore still flag where two interfaces do not,
        // whichever body-less spelling they use. Splitting the same two classes
        // across files clears them, which is what shows the name is read at all.
        let dir = tempfile::tempdir().unwrap();
        let class = |header: &str, name: &str, member: &str| {
            format!("export {header} {name} {{\n{CLEAR_DATABASE_DOC}{member}\n}}\n")
        };
        let filler = write(&dir, "filler.ts", "export const x = 1;\n");
        let abstract_member = "abstract clearDatabase(database?: string): Promise<void>";
        let abstract_pair = write(
            &dir,
            "runners.ts",
            &format!(
                "{}{}",
                class("abstract class", "BaseRunner", abstract_member),
                class("abstract class", "BaseBuilder", abstract_member)
            ),
        );
        let diags = run(&[&abstract_pair, &filler]);
        assert_eq!(diags.len(), 1, "a class body grants no owner, so the copies still flag");
        assert!(diags[0].message.contains("Near-duplicate comment"));
        // An overload signature needs an implementation signature to be valid
        // TypeScript. The implementation carries no doc-comment, so it is not a
        // comment site and does not change the count.
        let overload_member = "clearDatabase(database?: string): Promise<void>;\n\
                               clearDatabase(database?: string): Promise<void> {\n\
                               return Promise.resolve()\n}";
        let overload_pair = write(
            &dir,
            "builders.ts",
            &format!(
                "{}{}",
                class("class", "BaseRunner", overload_member),
                class("class", "BaseBuilder", overload_member)
            ),
        );
        assert_eq!(
            run(&[&overload_pair, &filler]).len(),
            1,
            "a class overload signature inherits no owner either"
        );
        let split_a = write(&dir, "a.ts", &class("class", "BaseRunner", overload_member));
        let split_b = write(&dir, "b.ts", &class("class", "BaseBuilder", overload_member));
        assert!(
            run(&[&split_a, &split_b]).is_empty(),
            "the same signature is name-attributed; only the owner is missing"
        );
    }

    #[test]
    fn still_flags_same_computed_key_method_docs_across_files() {
        // A computed key names nothing stable: both methods below spell their key
        // `[key]`, yet `key` holds a different string in each file. Reading the key
        // expression as a name would exempt two unrelated methods.
        let dir = tempfile::tempdir().unwrap();
        let iface = |key: &str, name: &str| {
            format!(
                "const key = '{key}'\nexport interface {name} {{\n{CLEAR_DATABASE_DOC}\
                 [key](database?: string): Promise<void>\n}}\n"
            )
        };
        let a = write(&dir, "Runner.ts", &iface("run", "Runner"));
        let b = write(&dir, "Encoder.ts", &iface("encode", "Encoder"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a computed key must not be read as a declaration name");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn overload_run_in_one_owner_is_exempt_whatever_else_is_scanned() {
        // Two readings of one run of overload signatures. It is exempt on its own
        // (#8210: one name in one scope is one item, and TypeScript has no syntax
        // for sharing a JSDoc block across its signatures), and — the point of
        // #8373 — that verdict does not move when an unrelated file joins the scan
        // or when a same-named partner sorts before or after it.
        let dir = tempfile::tempdir().unwrap();
        let iface = write(
            &dir,
            "MethodOverloads.ts",
            &format!(
                "export interface MethodOverloads {{\n{CLEAR_DATABASE_DOC}\
                 clearDatabase(a: string): this\n{CLEAR_DATABASE_DOC}\
                 clearDatabase(a: string, b: string): this\n}}\n"
            ),
        );
        let filler = write(&dir, "filler.ts", "export const x = 1;\n");
        assert!(run(&[&iface, &filler]).is_empty(), "an overload run documents one item");
        let implementation = format!(
            "export class Implementation {{\n{CLEAR_DATABASE_DOC}\
             clearDatabase(a: string): this {{\n    return this\n}}\n}}\n"
        );
        let early = write(&dir, "Early.ts", &implementation);
        assert!(run(&[&iface, &early]).is_empty(), "an earlier same-named partner changes nothing");
        let late = write(&dir, "ZLate.ts", &implementation);
        assert!(run(&[&iface, &late]).is_empty(), "nor does a later-sorting one");
    }

    #[test]
    fn still_flags_copy_pasted_jsdoc_on_two_methods_of_one_interface() {
        // Over-exclusion guard: two *differently named* methods of the *same*
        // interface sharing a doc is real duplication. The shared interface owner
        // keeps the parallel-member exemption from firing.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "QueryRunner.ts",
            &format!(
                "export interface QueryRunner {{\n{CLEAR_DATABASE_DOC}\
                 clearDatabase(database?: string): Promise<void>\n{CLEAR_DATABASE_DOC}\
                 dropDatabase(database?: string): Promise<void>\n}}\n"
            ),
        );
        let b = write(&dir, "filler.ts", "export const x = 1;\n");
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "copy-pasted JSDoc on two methods of one interface is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_copy_pasted_jsdoc_across_differently_named_interface_and_class_methods() {
        // Over-exclusion guard: the exemption stays name-keyed. The same doc pasted
        // onto an interface method and onto an unrelated class method in another
        // file describes two different contracts and must still flag.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "QueryRunner.ts",
            &format!(
                "export interface QueryRunner {{\n{CLEAR_DATABASE_DOC}\
                 clearDatabase(database?: string): Promise<void>\n}}\n"
            ),
        );
        let b = write(
            &dir,
            "EntityCache.ts",
            &format!(
                "export class EntityCache {{\n{CLEAR_DATABASE_DOC}\
                 purgeEntries(): void {{}}\n}}\n"
            ),
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "the same JSDoc on unrelated methods is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
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
    fn ignores_doc_citation_pointers() {
        // Regression (#4000): a one-line comment whose body cites a canonical
        // doc is the dedup remedy this rule recommends, not a copy-paste smell.
        // Without the citation guard each of these three identical pointers
        // would flag the others (17 normalized words, distinctive opener).
        let dir = tempfile::tempdir().unwrap();
        let line = "// Warm-cache loader, skip on in-page \"stay\" — see docs/agents/frontend-patterns.md (SSR-prefetch, #686).\nexport const loader = 1;\n";
        let a = write(&dir, "produits.tsx", line);
        let b = write(&dir, "gammes.tsx", line);
        let c = write(&dir, "cabinets.tsx", line);
        assert!(run(&[&a, &b, &c]).is_empty());
    }

    #[test]
    fn ignores_rationale_in_sibling_file() {
        // A `rationale in <relative source path>` pointer is also a citation.
        let dir = tempfile::tempdir().unwrap();
        let line = "// Head builder kept in its own file (not the route) for jsdom-safe testing — rationale in laboratories/head.ts.\nexport const head = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn ignores_url_citation() {
        let dir = tempfile::tempdir().unwrap();
        let line = "// Retry budget mirrors the upstream gateway window documented at https://example.com/runbooks/retries so the two never disagree.\nexport const retry = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn still_flags_duplicate_naming_a_path_without_an_adjacent_cue() {
        // The guard must stay surgical: a genuinely copy-pasted rationale that
        // merely names a source file — with no `see` / `rationale in` cue next
        // to the path — is real duplication and must still be flagged.
        let dir = tempfile::tempdir().unwrap();
        let line = "// The migration runner walks db/migrate.ts entries and applies them in lexical order so schema changes stay reproducible across every deployment environment.\nexport const run = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicate prose naming a path is still a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_long_rationale_that_merely_mentions_a_doc() {
        // Over-exclusion guard: a restated rationale that *names* a `.md` doc
        // without an adjacent citation cue (`mirrored in rejected.md`, not
        // `see rejected.md`) is genuine duplication — the doc mention must not
        // exempt the whole comment.
        let dir = tempfile::tempdir().unwrap();
        let line = "// Validate the upload before writing to disk and record the outcome in the audit log; the same failure taxonomy is mirrored in rejected.md so they never disagree.\nexport const v = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a doc named without a cue does not exempt a rationale");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_long_rationale_containing_a_docs_path_without_cue() {
        // A `docs/…` path embedded in restated prose (not introduced by a cue)
        // is still a duplicated rationale.
        let dir = tempfile::tempdir().unwrap();
        let line = "// Generated fixtures land under docs/examples during the build and are copied into the bundle so the playground and the published guide always show identical output.\nexport const f = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        assert_eq!(run(&[&a, &b]).len(), 1);
    }

    #[test]
    fn cue_verb_far_from_path_does_not_exempt() {
        // `see` used as an ordinary verb, not adjacent to the path, must not
        // latch onto an incidental source path and silence a real duplicate.
        let dir = tempfile::tempdir().unwrap();
        let line = "// Here we see the cache invalidation in lib/cache.ts kick in only after the lease expires, which keeps stale reads from leaking into the response.\nexport const c = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        assert_eq!(run(&[&a, &b]).len(), 1, "distant cue verb must not exempt");
    }

    #[test]
    fn still_flags_rationale_describing_where_source_code_lives() {
        // A weak verb cue (`defined`/`described`/`noted`) before a *source* path
        // describes where code lives — ordinary prose, not a citation — so a
        // copy-pasted rationale using it must still be flagged.
        let dir = tempfile::tempdir().unwrap();
        let defined = "// Default theme values defined in config/app.ts are then merged with the user overrides before the very first paint.\nexport const t = 1;\n";
        let a = write(&dir, "a.ts", defined);
        let b = write(&dir, "b.ts", defined);
        assert_eq!(run(&[&a, &b]).len(), 1, "`defined in <src>` is prose, not a citation");

        let dir = tempfile::tempdir().unwrap();
        let described = "// The eviction policy described in lib/cache.ts drops the oldest entry once the lease window has fully closed.\nexport const c = 1;\n";
        let a = write(&dir, "a.ts", described);
        let b = write(&dir, "b.ts", described);
        assert_eq!(run(&[&a, &b]).len(), 1, "`described in <src>` is prose, not a citation");
    }

    #[test]
    fn ignores_weak_cue_pointing_at_a_doc() {
        // The same weak verb cue against a *doc* reference is a real pointer.
        let dir = tempfile::tempdir().unwrap();
        let line = "// Auth handshake ordering is documented in docs/security/auth.md so the client and server stay in lockstep across releases.\nexport const h = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        assert!(run(&[&a, &b]).is_empty());
    }

    #[test]
    fn ignores_oxlint_disable_directive_block() {
        // Regression (#3843): a lint-disable directive is byte-identical in every
        // file that needs it by an external contract, so "keep one, point the rest
        // at it" cannot apply. Two files independently disabling the same rules is
        // the directive working as designed, not copy-paste drift. The justification
        // is long enough to clear the word/entropy gates, so only the directive
        // exclusion keeps it out.
        let dir = tempfile::tempdir().unwrap();
        let line = "/* oxlint-disable ban-types, no-empty-object-type -- the TaggedError public API intentionally requires an empty object literal here so subclasses stay structurally compatible */\nexport const e = 1;\n";
        let a = write(&dir, "forbidden.ts", line);
        let b = write(&dir, "index.ts", line);
        assert!(run(&[&a, &b]).is_empty(), "lint-disable directives must not be flagged");
    }

    #[test]
    fn ignores_eslint_disable_next_line_directive() {
        // The `-next-line` suffix is covered by starts-with on the canonical token.
        let dir = tempfile::tempdir().unwrap();
        let line = "// eslint-disable-next-line @typescript-eslint/no-explicit-any -- the upstream third-party callback signature is typed as any and we cannot widen it from here\nexport const f = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        assert!(run(&[&a, &b]).is_empty(), "eslint-disable-next-line must not be flagged");
    }

    #[test]
    fn ignores_use_no_memo_pragma_trailing_comment() {
        // Regression (#3843): a comment trailing a module-top string-literal pragma
        // is mandated identical across every file carrying the pragma, so it cannot
        // be deduplicated. The comment text alone is ordinary prose long enough to
        // flag — only the preceding `"use no memo"` pragma keeps it out.
        let dir = tempfile::tempdir().unwrap();
        let line = "\"use no memo\"; // RHF register and the formState proxy break under the React Compiler so this whole module opts out of memoization to keep the form bindings stable\nexport const g = 1;\n";
        let a = write(&dir, "team-form.tsx", line);
        let b = write(&dir, "lab-form.tsx", line);
        assert!(run(&[&a, &b]).is_empty(), "comment trailing a pragma literal must not be flagged");
    }

    #[test]
    fn still_flags_comment_mentioning_a_directive_mid_sentence() {
        // Over-exclusion guard: a free-form comment that merely *mentions* a tool
        // directive mid-sentence (not at the start) is ordinary prose. Starts-with,
        // not contains, keeps a duplicated such rationale flagged.
        let dir = tempfile::tempdir().unwrap();
        let line = "// Production builds intentionally trigger one oxlint-disable comment because the generated vendor bundle ships third-party types we cannot edit without breaking the upstream package contract.\nexport const h = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a mid-sentence directive mention is prose, not a directive");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_comment_trailing_ordinary_code() {
        // Over-exclusion guard: a trailing comment after arbitrary code (not a
        // pragma literal) is not exempt — only the recognized pragma statements are.
        let dir = tempfile::tempdir().unwrap();
        let line = "const config = loadConfig(); // The config loader resolves environment overrides before defaults so production secrets always win over the committed development fallbacks here.\nexport const i = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a comment trailing real code is not a pragma trailer");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_doc_comments_repeated_across_cfg_if_arms() {
        // Regression (#4839): a `cfg_if!` macro defines the same private type alias
        // for several mutually-exclusive backends; only one arm compiles, so the
        // identical doc-comment across arms is conditional-compilation boilerplate,
        // not copy-paste drift. The doc is long/distinctive enough to clear the
        // word and entropy gates, so only the cfg-conditional guard keeps it out.
        let dir = tempfile::tempdir().unwrap();
        let content = "\
cfg_if! {
    if #[cfg(curve25519_dalek_backend = \"fiat\")] {
        /// An `UnpackedScalar` represents an element of the field GF(l), optimized for speed.
        ///
        /// This is a type alias for one of the scalar types in the `backend` module.
        #[cfg(curve25519_dalek_bits = \"32\")]
        type UnpackedScalar = backend::serial::fiat_u32::scalar::Scalar29;

        /// An `UnpackedScalar` represents an element of the field GF(l), optimized for speed.
        ///
        /// This is a type alias for one of the scalar types in the `backend` module.
        #[cfg(curve25519_dalek_bits = \"64\")]
        type UnpackedScalar = backend::serial::fiat_u64::scalar::Scalar52;
    } else if #[cfg(curve25519_dalek_bits = \"64\")] {
        /// An `UnpackedScalar` represents an element of the field GF(l), optimized for speed.
        ///
        /// This is a type alias for one of the scalar types in the `backend` module.
        type UnpackedScalar = backend::serial::u64::scalar::Scalar52;
    }
}
";
        let a = write(&dir, "scalar.rs", content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(
            run(&[&a, &b]).is_empty(),
            "doc-comments repeated across cfg_if! arms must not be flagged"
        );
    }

    #[test]
    fn ignores_doc_comments_on_parallel_cfg_gated_items() {
        // The standalone `#[cfg(...)]` shape: the same item is defined twice with
        // mutually-exclusive `#[cfg]` predicates, each carrying the same doc. Only
        // one compiles, so the repetition is structural, not a smell.
        let dir = tempfile::tempdir().unwrap();
        let content = "\
/// The platform clock source selected at compile time for the timing subsystem.
#[cfg(target_os = \"linux\")]
type Clock = LinuxClock;

/// The platform clock source selected at compile time for the timing subsystem.
#[cfg(target_os = \"macos\")]
type Clock = MacClock;
";
        let a = write(&dir, "clock.rs", content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(
            run(&[&a, &b]).is_empty(),
            "doc-comments on parallel #[cfg]-gated items must not be flagged"
        );
    }

    #[test]
    fn still_flags_duplicate_docs_on_unconditional_items() {
        // Over-exclusion guard: an ordinary `#[derive]`/`#[inline]` attribute (not
        // `#[cfg]`) is no conditional-compilation signal, so a genuinely duplicated
        // doc-comment on such items in unconditional scope must still flag. The two
        // declarations carry different names so the cross-file parallel-implementation
        // exemption (same `decl_name`) does not apply — this isolates the cfg signal.
        let dir = tempfile::tempdir().unwrap();
        let doc = "/// Builds the canonical pagination defaults derived from the shared schema so every list stays consistent.\n";
        let a = write(&dir, "a.rs", &format!("{doc}#[inline]\npub fn one() {{}}\n"));
        let b = write(&dir, "b.rs", &format!("{doc}#[inline]\npub fn two() {{}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicated docs on non-cfg items are still a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_doc_comment_templates_inside_macro_rules_body_issue_7072() {
        // Regression (#7072): nom defines the same `ints!` macro in
        // `character/streaming.rs` and `character/complete.rs`; the doc-comment
        // inside each `macro_rules!` body is a template stamped onto every
        // function the macro expands. It must live inline in the macro arm — it
        // cannot be lifted into a canonical doc — so the identical template across
        // the two parallel files is not a copy-paste smell. The doc is long and
        // distinctive enough to clear the word and entropy gates (it flags outside
        // a macro in the sibling test), so only the macro-body skip keeps it out.
        let dir = tempfile::tempdir().unwrap();
        let content = "\
#[doc(hidden)]
macro_rules! ints {
    ($($t:tt)+) => {
        $(
        /// Builds the canonical pagination defaults derived from the shared schema so every list stays consistent everywhere.
        pub fn $t() {}
        )+
    }
}
";
        let a = write(&dir, "streaming.rs", content);
        let b = write(&dir, "complete.rs", content);
        assert!(
            run(&[&a, &b]).is_empty(),
            "doc-comment templates inside macro_rules! bodies must not flag"
        );
    }

    #[test]
    fn still_flags_duplicate_doc_outside_macro_in_file_with_macros_issue_7072() {
        // Over-exclusion guard for #7072: the macro-body skip is structural (an
        // ancestor is `macro_definition`), not a whole-file exemption. The same
        // doc used as a template above still flags when copy-pasted onto two
        // differently-named top-level functions OUTSIDE any macro — even in a file
        // that also defines a macro.
        let dir = tempfile::tempdir().unwrap();
        let doc = "/// Builds the canonical pagination defaults derived from the shared schema so every list stays consistent everywhere.\n";
        let macro_def = "macro_rules! noop { () => {}; }\n";
        let a = write(&dir, "a.rs", &format!("{macro_def}{doc}pub fn build_admin_list() {{}}\n"));
        let b = write(&dir, "b.rs", &format!("{macro_def}{doc}pub fn build_lab_section() {{}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a duplicate doc outside the macro must still flag");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_parallel_enum_variant_docs_intra_file() {
        // Regression (#5316): flate2's `FlushCompress` and `FlushDecompress` are
        // parallel enums for the two directions of one zlib operation, so their
        // same-named variants (`None`/`Sync`) carry identical doc-comments
        // describing the same flush mode. Both enums live in one file, so this is
        // not the cross-file exemption — distinct enum owners keep it out.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
    /// A typical parameter for passing to compression and decompression functions,
    /// this indicates that the underlying stream decides how much data to accumulate
    /// before producing output in order to maximize the resulting compression ratio.\n";
        let content = format!(
            "pub enum FlushCompress {{\n{doc}    None = 0,\n}}\n\
             pub enum FlushDecompress {{\n{doc}    None = 0,\n}}\n"
        );
        let a = write(&dir, "mem.rs", &content);
        let b = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(
            run(&[&a, &b]).is_empty(),
            "identical docs on same-named variants of different enums must not flag"
        );
    }

    #[test]
    fn still_flags_duplicate_doc_on_differently_named_enum_variants() {
        // Over-exclusion guard for #5316: the variant exemption is name-keyed. An
        // identical doc-comment copy-pasted onto two *differently named* variants
        // (even across different enums) is real duplication and must still flag.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
    /// A typical parameter for passing to compression and decompression functions,
    /// this indicates that the underlying stream decides how much data to accumulate
    /// before producing output in order to maximize the resulting compression ratio.\n";
        let a = write(&dir, "a.rs", &format!("pub enum A {{\n{doc}    Alpha = 0,\n}}\n"));
        let b = write(&dir, "b.rs", &format!("pub enum B {{\n{doc}    Beta = 0,\n}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "identical doc on differently-named variants is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn ignores_repeated_safety_invariant_comments_issue_6264() {
        // Regression (#6264): tokio-rs/bytes restates the same `// SAFETY:`
        // invariant above two unsafe blocks that both call `shallow_clone`. The
        // rationale is identical because the invariant is the same — restating
        // it at every call site is idiomatic local safety documentation, not a
        // copy-paste smell. The comment is long/distinctive enough to clear the
        // word and entropy gates, so only the SAFETY-marker exclusion keeps it out.
        // The two enclosing functions are named differently so the same-named-
        // method exemption (#6829) cannot also produce the emptiness under test —
        // the SAFETY-marker exclusion must be the sole reason these do not flag.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "bytes_mut.rs",
            "\
fn split_off(&mut self, at: usize) {
    // SAFETY: `shallow_clone` increments the reference count (or promotes to
    // shared) and returns a bitwise copy of the handle. The caller immediately
    // adjusts both handles so they represent disjoint regions of one buffer.
    let other = self.shallow_clone();
}
",
        );
        let b = write(
            &dir,
            "bytes.rs",
            "\
fn split_to(&mut self, at: usize) {
    // SAFETY: `shallow_clone` increments the reference count (or promotes to
    // shared) and returns a bitwise copy of the handle. The caller immediately
    // adjusts both handles so they represent disjoint regions of one buffer.
    let other = self.shallow_clone();
}
",
        );
        assert!(
            run(&[&a, &b]).is_empty(),
            "repeated // SAFETY: invariant comments must not flag"
        );
    }

    #[test]
    fn still_flags_repeated_plain_prose_without_safety_marker_issue_6264() {
        // Over-exclusion guard for #6264: the same shape WITHOUT a `// SAFETY:`
        // marker is ordinary duplicated prose copy-pasted across files and must
        // still flag — the exclusion is keyed on the structural safety marker.
        // The two functions are named differently so the same-named-enclosing-
        // method exemption (#6829) cannot mask the SAFETY-marker signal under test.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.rs",
            "\
fn split_off(&mut self, at: usize) {
    // `shallow_clone` increments the reference count (or promotes to shared)
    // and returns a bitwise copy of the handle. The caller immediately adjusts
    // both handles so they represent disjoint regions of one buffer.
    let other = self.shallow_clone();
}
",
        );
        let b = write(
            &dir,
            "b.rs",
            "\
fn split_to(&mut self, at: usize) {
    // `shallow_clone` increments the reference count (or promotes to shared)
    // and returns a bitwise copy of the handle. The caller immediately adjusts
    // both handles so they represent disjoint regions of one buffer.
    let other = self.shallow_clone();
}
",
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "duplicated plain prose without a SAFETY marker is still a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn skips_numeric_column_index_labels_issue_6273() {
        // Regression (#6273): hyperium/http labels each 256-byte lookup table
        // with a column-index header (`//  0  1  2 …`). The same header recurs
        // across the four tables, but it is an all-integer data-structure
        // annotation, not prose — there is nothing to lift into a canonical doc.
        // A 16-wide header is used so the comment clears `min_words` and would
        // pass the entropy gate (8 distinct numeric prefix words) without the
        // all-numeric guard, proving the guard — not the length gate — exempts it.
        let dir = tempfile::tempdir().unwrap();
        let header = "// 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15\nconst TABLE: [u8; 16] = [];\n";
        let a = write(&dir, "method.rs", header);
        let b = write(&dir, "name.rs", header);
        assert!(run(&[&a, &b]).is_empty(), "numeric column-index labels must not flag");

        // Multi-digit constant rows (`// 100 200 300 …`) are equally all-numeric.
        let magic = "// 100 200 300 400 500 600 700 800 900 1000 1100 1200 1300 1400 1500\nconst ROW: u32 = 0;\n";
        let c = write(&dir, "c.rs", magic);
        let d = write(&dir, "d.rs", magic);
        assert!(run(&[&c, &d]).is_empty(), "all-numeric constant rows must not flag");
    }

    #[test]
    fn still_flags_label_mixing_a_word_with_numbers_issue_6273() {
        // Over-exclusion guard for #6273: a label that carries any non-numeric
        // word (`row`) is not an all-numeric annotation, so it stays subject to
        // duplicate detection and a copy-pasted one still flags.
        let dir = tempfile::tempdir().unwrap();
        let mixed = "// row 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14\nconst TABLE: [u8; 15] = [];\n";
        let a = write(&dir, "a.rs", mixed);
        let b = write(&dir, "b.rs", mixed);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a label mixing a word with numbers is still checked");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn all_numeric_label_predicate_edges_issue_6273() {
        // The structural predicate: all integer tokens skip; an empty word list
        // (blank comment) is NOT all-numeric, so it cannot vacuously skip; any
        // alphabetic token disqualifies the whole label.
        let nums = |xs: &[&str]| -> Vec<String> { xs.iter().map(|s| s.to_string()).collect() };
        assert!(is_all_numeric_label(&nums(&["0", "1", "15"])));
        assert!(is_all_numeric_label(&nums(&["100", "200", "300"])));
        assert!(!is_all_numeric_label(&[]), "an empty word list is not all-numeric");
        assert!(!is_all_numeric_label(&nums(&["row", "0", "1"])));
        assert!(!is_all_numeric_label(&nums(&["0x1f"])), "hex is not a decimal token");
    }

    /// The drizzle inline body comment: a plain `//` describing the cache-mutate
    /// step, long and distinctive enough to clear the word and entropy gates so
    /// only an enclosing-name exemption can keep a cross-file pair out.
    const CACHE_MUTATE_NOTE: &str = "    // For mutate queries, we should query the database, wait for a response, and then perform invalidation of the dependent cache entries here.";

    #[test]
    fn ignores_parallel_inline_comments_in_same_named_methods_issue_6829() {
        // Regression (#6829): drizzle-orm implements the same caching protocol
        // once per dialect package, so the identical inline comment sits inside
        // the same-named `queryWithCache` method of each dialect's session class.
        // The comment describes the same algorithm step, not a copy-paste smell.
        let dir = tempfile::tempdir().unwrap();
        let pg = format!(
            "class PgSession {{\n  queryWithCache() {{\n{CACHE_MUTATE_NOTE}\n    if (this.kind === 'insert') {{ return 1; }}\n  }}\n}}\n"
        );
        let gel = format!(
            "class GelSession {{\n  queryWithCache() {{\n{CACHE_MUTATE_NOTE}\n    if (this.kind === 'insert') {{ return 2; }}\n  }}\n}}\n"
        );
        let a = write(&dir, "pg-session.ts", &pg);
        let b = write(&dir, "gel-session.ts", &gel);
        assert!(
            run(&[&a, &b]).is_empty(),
            "inline comment inside same-named methods across files must not flag"
        );
    }

    #[test]
    fn still_flags_inline_comment_in_differently_named_methods_across_files() {
        // The exemption is keyed on the enclosing method name: the same inline
        // comment inside two *differently named* methods across files is ordinary
        // copy-pasted prose and must still flag.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "a.ts",
            &format!("class PgSession {{\n  queryWithCache() {{\n{CACHE_MUTATE_NOTE}\n    return 1;\n  }}\n}}\n"),
        );
        let b = write(
            &dir,
            "b.ts",
            &format!("class GelSession {{\n  runWithCache() {{\n{CACHE_MUTATE_NOTE}\n    return 2;\n  }}\n}}\n"),
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "identical inline comment in differently-named methods is a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_inline_comment_duplicated_within_one_file_issue_6829() {
        // The exemption is cross-file only: the same inline comment inside two
        // same-named methods of two classes in ONE file is a botched copy-paste
        // and must still flag, even though the enclosing method names match.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "class PgSession {{\n  queryWithCache() {{\n{CACHE_MUTATE_NOTE}\n    return 1;\n  }}\n}}\n\
             class GelSession {{\n  queryWithCache() {{\n{CACHE_MUTATE_NOTE}\n    return 2;\n  }}\n}}\n"
        );
        let a = write(&dir, "sessions.ts", &content);
        let filler = write(&dir, "filler.ts", "export const z = 1;\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "intra-file duplicate inline comment is still a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_free_floating_duplicate_inline_comment_across_files_issue_6829() {
        // A duplicate comment with NO enclosing named function earns no parallel
        // exemption (its enclosing name is `None`), so a copy-pasted free-floating
        // rationale across files still flags.
        let dir = tempfile::tempdir().unwrap();
        let line = "// For mutate queries, we should query the database, wait for a response, and then perform invalidation of the dependent cache entries here.\nexport const x = 1;\n";
        let a = write(&dir, "a.ts", line);
        let b = write(&dir, "b.ts", line);
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "free-floating duplicate inline comment is still a smell");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    // The valibot JSDoc block from #8210, long enough to clear `min_words`.
    const MIN_VALUE_DOC: &str = "\
/**
 * Builds a minimum value validation action for numeric datasets.
 *
 * @param requirement The minimum value the input must reach.
 *
 * @returns A min value action.
 */
";

    #[test]
    fn ignores_ambient_declare_function_jsdoc_across_files_issue_8210() {
        // Regression (#8210): `export declare function` nests its
        // `function_signature` under an `ambient_declaration` inside the
        // `export_statement`. Only the outer wrapper used to be unwrapped, so the
        // signature named nothing and every exemption keyed on a declaration name
        // died. The `.d.ts` spelling and the implemented spelling must agree.
        let dir = tempfile::tempdir().unwrap();
        let ambient = format!("{MIN_VALUE_DOC}export declare function maxValue<T>(r: T): T;\n");
        let a = write(&dir, "one.ts", &ambient);
        let b = write(&dir, "two.ts", &ambient);
        assert!(run(&[&a, &b]).is_empty(), "an ambient declaration is a named declaration");

        let implemented = format!("{MIN_VALUE_DOC}export function maxValue<T>(r: T): T {{ return r }}\n");
        let c = write(&dir, "three.ts", &implemented);
        let d = write(&dir, "four.ts", &implemented);
        assert!(run(&[&c, &d]).is_empty(), "control: the implemented spelling was already clean");
    }

    #[test]
    fn ignores_overload_signature_jsdoc_on_async_sync_twins_issue_8210() {
        // #7044's sync/async twin exemption expressed as overload signatures: the
        // JSDoc sits on a `function_signature`, and the two names differ only by
        // the camelCase `Async` suffix.
        let dir = tempfile::tempdir().unwrap();
        let twin = |name: &str| {
            format!(
                "{MIN_VALUE_DOC}export function {name}<T>(r: T): T;\n\
                 export function {name}<T>(r: T, m?: string): T {{ return r }}\n"
            )
        };
        let a = write(&dir, "one.ts", &twin("maxValue"));
        let b = write(&dir, "two.ts", &twin("maxValueAsync"));
        assert!(run(&[&a, &b]).is_empty(), "overloads of sync/async twins must not flag");
    }

    #[test]
    fn ignores_overload_run_of_one_top_level_function_issue_8210() {
        // Two overload signatures of one exported function in one file. They share
        // a name and a scope, so they are one item declared twice — and TypeScript
        // resolves the doc per selected signature, so neither block can be dropped.
        let dir = tempfile::tempdir().unwrap();
        let overloads = format!(
            "{MIN_VALUE_DOC}export function minValue<T>(requirement: T): T;\n\
             {MIN_VALUE_DOC}export function minValue<T, M>(requirement: T, message: M): T;\n\
             export function minValue<T, M>(requirement: T, message?: M): T {{ return requirement }}\n"
        );
        let a = write(&dir, "minValue.ts", &overloads);
        let filler = write(&dir, "filler.ts", "export const x = 1;\n");
        assert!(run(&[&a, &filler]).is_empty(), "an overload run documents one function");
    }

    #[test]
    fn still_flags_copy_pasted_jsdoc_on_two_top_level_functions_issue_8210() {
        // Over-exclusion guard: the same JSDoc on two *differently named* exported
        // functions in one file is the rule's core case and must survive the
        // intra-file relaxation.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "{MIN_VALUE_DOC}export function minValue<T>(r: T): T {{ return r }}\n\
             {MIN_VALUE_DOC}export function trimStart(r: string): string {{ return r }}\n"
        );
        let a = write(&dir, "actions.ts", &content);
        let filler = write(&dir, "filler.ts", "export const x = 1;\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "one JSDoc on two unrelated functions is a copy-paste");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_plain_comment_copy_pasted_above_two_overload_signatures_issue_8210() {
        // The relaxation keeps the doc-comment gate: a plain `//` above a
        // declaration is incidental prose, never its documentation, so a
        // copy-pasted one still flags even between two signatures of one function.
        let dir = tempfile::tempdir().unwrap();
        let note = "// Kept here because the runner reads the requirement before the message is resolved at call time.\n";
        let content = format!(
            "{note}export function minValue<T>(r: T): T;\n\
             {note}export function minValue<T, M>(r: T, m: M): T;\n\
             export function minValue<T, M>(r: T, m?: M): T {{ return r }}\n"
        );
        let a = write(&dir, "minValue.ts", &content);
        let filler = write(&dir, "filler.ts", "export const x = 1;\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "a plain `//` earns no declaration attribution");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    // valibot's `'~run'` member doc, carried by every schema's inline result type.
    const RUN_MEMBER_DOC: &str = "    /**
     * Parses unknown input values against this schema definition.
     *
     * @param dataset The input dataset to parse.
     * @param config The parse configuration.
     *
     * @returns The typed output dataset.
     */
";

    #[test]
    fn ignores_same_named_member_docs_under_two_anonymous_type_literals_issue_8210() {
        // Two overloads of `omit` each return an inline `{ … }` declaring the same
        // `'~run'` member. A type literal has no name to expose as an owner, but
        // two *distinct* literals are two distinct owners just as two named
        // interfaces are, which is what the owned-member exemption actually tests.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "export declare function omit<T>(keys: readonly string[]): {{\n\
             {RUN_MEMBER_DOC}  readonly run: (dataset: unknown) => unknown\n}};\n\
             export declare function omit<T, M>(keys: readonly string[], message: M): {{\n\
             {RUN_MEMBER_DOC}  readonly run: (dataset: unknown) => unknown\n}};\n"
        );
        let a = write(&dir, "omit.ts", &content);
        let filler = write(&dir, "filler.ts", "export const x = 1;\n");
        assert!(run(&[&a, &filler]).is_empty(), "two type literals are two owners");
    }

    #[test]
    fn still_flags_copy_pasted_member_docs_inside_one_anonymous_type_literal_issue_8210() {
        // Over-exclusion guard: two members of the *same* literal share an owner,
        // so the exemption cannot reach them.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "export declare function omit<T>(keys: readonly string[]): {{\n\
             {RUN_MEMBER_DOC}  readonly run: (dataset: unknown) => unknown\n\
             {RUN_MEMBER_DOC}  readonly validate: (dataset: unknown) => unknown\n}};\n"
        );
        let a = write(&dir, "omit.ts", &content);
        let filler = write(&dir, "filler.ts", "export const x = 1;\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "two members of one literal are a copy-paste");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    // arrow-rs's `Date32Type` temporal API doc, repeated per `add_*` entry point.
    const ADD_YEAR_MONTHS_DOC: &str = "    /// Adds the given IntervalYearMonthType to an arrow Date32Type value.
    ///
    /// # Arguments
    ///
    /// * `date` - The date on which to perform the operation here
    /// * `delta` - The interval to add to that date value
";

    #[test]
    fn ignores_fallible_twin_docs_in_one_impl_block_issue_8059() {
        // Regression (#8059): apache/arrow-rs documents `add_year_months` and its
        // infallible/fallible twin `add_year_months_opt` in one `impl` block. Each
        // is a public entry point that rustc's `missing_docs` requires a doc on,
        // and rustdoc renders one page per item, so neither can cite the other.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "impl Date32Type {{\n\
             {ADD_YEAR_MONTHS_DOC}    pub fn add_year_months(date: i32, delta: i32) -> i32 {{ date + delta }}\n\
             {ADD_YEAR_MONTHS_DOC}    pub fn add_year_months_opt(date: i32, delta: i32) -> Option<i32> {{ Some(date) }}\n}}\n"
        );
        let a = write(&dir, "types.rs", &content);
        let filler = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(run(&[&a, &filler]).is_empty(), "an `_opt` twin in one impl must not flag");

        // The same doc over an unrelated sibling is a copy-paste, which is what
        // shows the emptiness above comes from the name relation and not from the
        // fixture falling short of the word gate.
        let unrelated = content.replace("add_year_months_opt", "format_iso_week");
        let b = write(&dir, "unrelated.rs", &unrelated);
        assert_eq!(run(&[&b, &filler]).len(), 1, "an unrelated sibling still flags");
    }

    // ripgrep's `Matcher` trait doc, repeated across its iteration entry points.
    const CAPTURES_ITER_DOC: &str = "    /// Executes the given function over successive non-overlapping matches
    /// in `haystack` with capture groups extracted from each single match. If no
    /// match exists, then the given function is never called anywhere.
";

    #[test]
    fn ignores_variant_named_siblings_of_one_api_surface_issue_8311() {
        // Regression (#8311): `captures_iter` and `captures_iter_at` are two entry
        // points to one operation declared side by side in one `trait`. Their
        // shared root is two segments — under `MIN_VARIANT_ROOT_SEGMENTS` — but the
        // floor exists to rule out *unrelated* declarations, and two members of one
        // surface are related by construction.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "pub trait Matcher {{\n\
             {CAPTURES_ITER_DOC}    fn captures_iter(&self, haystack: &[u8]) -> bool;\n\
             {CAPTURES_ITER_DOC}    fn captures_iter_at(&self, haystack: &[u8], at: usize) -> bool;\n\
             {CAPTURES_ITER_DOC}    fn try_captures_iter_at(&self, haystack: &[u8], at: usize) -> bool;\n}}\n"
        );
        let a = write(&dir, "lib.rs", &content);
        let filler = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(run(&[&a, &filler]).is_empty(), "variant-named trait siblings must not flag");

        let unrelated = content.replace("captures_iter_at", "shutdown_pool");
        let b = write(&dir, "unrelated.rs", &unrelated);
        assert_eq!(run(&[&b, &filler]).len(), 2, "an unrelated sibling still flags");
    }

    #[test]
    fn ignores_variant_named_siblings_split_across_impl_blocks_issue_8311() {
        // One type's methods are routinely split across several `impl` blocks to
        // vary the trait bounds — ripgrep's `Summary::new` and
        // `Summary::new_no_color`. The surface is the type, not the block, so the
        // two are still siblings.
        let dir = tempfile::tempdir().unwrap();
        let doc = "    /// Return a summary printer with a default configuration that writes the
    /// matches it is given to the writer it was handed at construction time.
";
        let content = format!(
            "impl<W: WriteColor> Summary<W> {{\n{doc}    pub fn new(wtr: W) -> Summary<W> {{ Summary }}\n}}\n\
             impl<W: io::Write> Summary<NoColor<W>> {{\n{doc}    pub fn new_no_color(wtr: W) -> Summary<NoColor<W>> {{ Summary }}\n}}\n"
        );
        let a = write(&dir, "summary.rs", &content);
        let filler = write(&dir, "filler.rs", "pub fn x() {}\n");
        assert!(run(&[&a, &filler]).is_empty(), "two impl blocks of one type are one surface");

        // Two *different* types are two surfaces, so the same pair of docs flags.
        let split = content.replace("impl<W: io::Write> Summary<NoColor<W>>", "impl<W: io::Write> Standard<NoColor<W>>");
        let b = write(&dir, "printers.rs", &split);
        assert_eq!(run(&[&b, &filler]).len(), 1, "two types are two surfaces");
    }

    #[test]
    fn still_flags_unrelated_names_as_siblings_of_one_impl_issue_8311() {
        // Over-exclusion guard: sibling membership alone must not exempt. Two
        // unrelated methods of one type carrying one rationale is the copy-paste
        // the rule exists to report.
        let dir = tempfile::tempdir().unwrap();
        let doc = "    /// Reads the configuration file from disk and returns its parsed contents to
    /// the caller, or an error describing precisely why the file could not be read.
";
        let content = format!(
            "impl Runner {{\n{doc}    pub fn load_config(&self) -> u8 {{ 0 }}\n\
             {doc}    pub fn spawn_worker_pool(&self) -> u8 {{ 1 }}\n}}\n"
        );
        let a = write(&dir, "runner.rs", &content);
        let filler = write(&dir, "filler.rs", "pub fn x() {}\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "unrelated sibling names are still a copy-paste");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_short_rooted_variant_names_outside_any_api_surface_issue_8311() {
        // The floor is waived for siblings only. The same two names as free
        // functions at file top level have no surface to make them relatives, so
        // `find` / `find_at` keeps flagging — and moving them into one trait
        // clears it, which is what shows the surface is the discriminator.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Locates the leftmost match this matcher accepts inside the haystack given to
/// it, or nothing at all when that haystack holds no match for the pattern.
";
        let free = format!("{doc}pub fn find(h: &[u8]) -> bool {{ true }}\n{doc}pub fn find_at(h: &[u8], at: usize) -> bool {{ true }}\n");
        let a = write(&dir, "free.rs", &free);
        let filler = write(&dir, "filler.rs", "pub fn x() {}\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "a one-segment root is too generic outside a surface");

        let indented = doc.replace("///", "    ///");
        let owned = format!(
            "pub trait Matcher {{\n{indented}    fn find(&self, h: &[u8]) -> bool;\n\
             {indented}    fn find_at(&self, h: &[u8], at: usize) -> bool;\n}}\n"
        );
        let b = write(&dir, "trait.rs", &owned);
        assert!(run(&[&b, &filler]).is_empty(), "the same pair inside one trait is exempt");
    }

    // typeorm's `QueryRunner.isReleased` contract, stated on the interface and
    // again on every class that implements it.
    const IS_RELEASED_DOC: &str = "    /**
     * Indicates if the connection for this query runner has been released.
     * Once it is released, the query runner cannot run any more queries.
     */
";

    #[test]
    fn ignores_interface_property_jsdoc_mirrored_by_class_field_issue_8372() {
        // Regression (#8372): the interface spells the member as a
        // `property_signature`, which earns attribution, and the implementing
        // class spells it as a `public_field_definition`, which did not — so the
        // interface reported its own implementation as a near-duplicate.
        let dir = tempfile::tempdir().unwrap();
        let iface = write(
            &dir,
            "QueryRunner.ts",
            &format!("export interface QueryRunner {{\n{IS_RELEASED_DOC}  readonly isReleased: boolean\n}}\n"),
        );
        let class = write(
            &dir,
            "MongoQueryRunner.ts",
            &format!("export class MongoQueryRunner implements QueryRunner {{\n{IS_RELEASED_DOC}  isReleased = false\n}}\n"),
        );
        assert!(run(&[&iface, &class]).is_empty(), "a class field mirroring its interface must not flag");
    }

    #[test]
    fn still_flags_duplicate_jsdoc_on_differently_named_class_fields_issue_8372() {
        // Over-exclusion guard: the attribution is name-keyed, so the same JSDoc
        // over two *differently named* fields is real duplication.
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            &dir,
            "A.ts",
            &format!("export class A {{\n{IS_RELEASED_DOC}  isReleased = false\n}}\n"),
        );
        let b = write(
            &dir,
            "B.ts",
            &format!("export class B {{\n{IS_RELEASED_DOC}  isDetached = false\n}}\n"),
        );
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "one JSDoc on two unrelated fields is a copy-paste");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_same_named_class_field_docs_across_classes_in_one_file_issue_8372() {
        // The asymmetry the fix deliberately keeps: two classes are two surfaces,
        // and a class body grants no owner, so a same-named field documented under
        // each of them in one file stays reportable — the boundary #6870 drew for
        // class methods, unchanged for class properties.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "export class MongoQueryRunner {{\n{IS_RELEASED_DOC}  isReleased = false\n}}\n\
             export class SqlQueryRunner {{\n{IS_RELEASED_DOC}  isReleased = false\n}}\n"
        );
        let a = write(&dir, "runners.ts", &content);
        let filler = write(&dir, "filler.ts", "export const x = 1;\n");
        let diags = run(&[&a, &filler]);
        assert_eq!(diags.len(), 1, "two classes in one file are two surfaces");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_plain_comment_above_a_class_field_issue_8372() {
        // A class property goes through the declaration path, which keeps the
        // doc-comment gate: a plain `//` above it earns no attribution.
        let dir = tempfile::tempdir().unwrap();
        let note = "  // Kept false until the pool hands the connection back, because the caller may still hold a cursor open.\n";
        let a = write(&dir, "A.ts", &format!("export class A {{\n{note}  isReleased = false\n}}\n"));
        let b = write(&dir, "B.ts", &format!("export class B {{\n{note}  isReleased = false\n}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "a plain `//` above a field earns no attribution");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn attributes_jsdoc_through_an_export_statement_issue_7125() {
        // `export function` / `export class` nest their declaration under an
        // `export_statement`; the name must be read through it.
        assert_eq!(
            first_comment_decl_name("/**\n * Caches the resolved schema output for later.\n */\nexport function cache() {}\n").as_deref(),
            Some("cache")
        );
        assert_eq!(
            first_comment_decl_name("/**\n * Caches the resolved schema output for later.\n */\nexport class Cache {}\n").as_deref(),
            Some("Cache")
        );
    }

    #[test]
    fn ignores_exported_sync_async_twin_jsdoc_issue_7125() {
        // End-to-end for the same seam, with a JSDoc past the word gate so the
        // comment actually reaches the exemption: valibot's `cache` / `cacheAsync`
        // twins, each `export function`.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/**
 * Produces a cached copy of the given schema so repeated parses reuse the work
 * already done for the first dataset the schema was handed.
 *
 * @param schema The schema to cache.
 *
 * @returns The cached schema.
 */
";
        let a = write(&dir, "cache.ts", &format!("{doc}export function cache(schema: number) {{ return schema }}\n"));
        let b = write(&dir, "cacheAsync.ts", &format!("{doc}export function cacheAsync(schema: number) {{ return schema }}\n"));
        assert!(run(&[&a, &b]).is_empty(), "exported sync/async twins must not flag");

        // The same doc on an unrelated export flags, so the block does clear the
        // word gate and the exemption is what produced the emptiness above.
        let c = write(&dir, "purge.ts", &format!("{doc}export function purgeEntries(schema: number) {{ return schema }}\n"));
        assert_eq!(run(&[&a, &c]).len(), 1, "an unrelated export still flags");
    }

    #[test]
    fn verdict_does_not_move_when_an_unrelated_third_file_joins_issue_8373() {
        // Regression (#8373): the partner used to be an argmax over the whole
        // bucket, so a third file could re-partner entries in files that had not
        // changed and silence a genuine copy-paste. `b_unrelated.rs` shares the doc
        // of `a_mirror.rs` / `z.rs` while documenting an unrelated function, so the
        // pair must be reported whether or not `a_mirror.rs` is scanned.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Fully-fixsliced AES-128 decryption with the InvShiftRows step completely
/// omitted so the four blocks are decrypted in place and in parallel across the
/// whole fixsliced state.
";
        let mirror = write(&dir, "src/a_mirror.rs", &format!("{doc}pub fn aes128_decrypt(x: u8) -> u8 {{ x }}\n"));
        let unrelated = write(&dir, "src/b_unrelated.rs", &format!("{doc}pub fn totally_unrelated_helper(x: u8) -> u8 {{ x }}\n"));
        let z = write(&dir, "src/z.rs", &format!("{doc}pub fn aes128_decrypt(y: u8) -> u8 {{ y }}\n"));

        let with_mirror = run(&[&mirror, &unrelated, &z]);
        assert!(
            with_mirror.iter().any(|d| d.path.ends_with("b_unrelated.rs")),
            "the unrelated copy is reported against the mirror it duplicates"
        );
        assert!(
            with_mirror.iter().any(|d| d.path.ends_with("z.rs")),
            "and the mirror is reported against the unrelated copy it also duplicates"
        );

        let without_mirror = run(&[&unrelated, &z]);
        assert_eq!(without_mirror.len(), 1, "removing an unrelated file does not silence the pair");
        assert!(without_mirror[0].path.ends_with("z.rs"));
    }

    #[test]
    fn cross_file_mirror_stays_exempt_without_an_unrelated_copy_issue_8373() {
        // Control for the quantifier change: a bucket holding only a legitimate
        // mirror set reports nothing, so the new "any earlier member" reading did
        // not turn every mirror into a diagnostic.
        let dir = tempfile::tempdir().unwrap();
        let doc = "\
/// Fully-fixsliced AES-128 decryption with the InvShiftRows step completely
/// omitted so the four blocks are decrypted in place and in parallel across the
/// whole fixsliced state.
pub fn aes128_decrypt(x: u8) -> u8 { x }
";
        let a = write(&dir, "src/a_mirror.rs", doc);
        let b = write(&dir, "src/z.rs", doc);
        assert!(run(&[&a, &b]).is_empty(), "a pure mirror set stays exempt");
    }

    // jiff's provenance pointers: one Rust test per upstream test262 case.
    fn tc39_pointer(case: &str) -> String {
        format!(
            "/// Source: https://github.com/tc39/test262/blob/29c6f7028a683b8259140e7d6352ae0ca6448a85/test/built-ins/Temporal/Duration/prototype/round/{case}.js\n\
             pub fn {} () -> bool {{ true }}\n",
            case.replace('-', "_")
        )
    }

    #[test]
    fn ignores_provenance_pointers_to_distinct_upstream_files_issue_8236() {
        // Regression (#8236): three pointers to three different upstream files.
        // Split on every non-alphanumeric character a URL becomes ~17 tokens of
        // which only the last differs, so a family of references under one base
        // scored near 1.0 on a measure that only reads the opening. Counted as one
        // word each, the comments carry one word of prose and never reach the gate.
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "{}{}{}",
            tc39_pointer("balance-negative-result"),
            tc39_pointer("balance-subseconds"),
            tc39_pointer("calendar-possibly-required")
        );
        let a = write(&dir, "src/lib.rs", &content);
        let filler = write(&dir, "src/filler.rs", "pub fn x() {}\n");
        assert!(run(&[&a, &filler]).is_empty(), "distinct provenance pointers are not copies");
    }

    #[test]
    fn distinct_url_citations_do_not_report_against_each_other_issue_8236() {
        // The same three references carried by prose long enough to clear the word
        // gate. Two comments that cite different targets are two different
        // pointers: there is no one comment to keep, so the remedy the diagnostic
        // proposes cannot apply.
        let dir = tempfile::tempdir().unwrap();
        let rationale = |case: &str| {
            format!(
                "// Ported verbatim from the upstream conformance suite so the rounding \
                 behaviour stays pinned to the specification text it came from originally: \
                 https://github.com/tc39/test262/blob/29c6f70/round/{case}.js\n\
                 pub fn {} () -> bool {{ true }}\n",
                case.replace('-', "_")
            )
        };
        let a = write(&dir, "a.rs", &rationale("balance-negative-result"));
        let b = write(&dir, "b.rs", &rationale("balance-subseconds"));
        assert!(run(&[&a, &b]).is_empty(), "different targets are different pointers");

        let same = rationale("balance-subseconds");
        let c = write(&dir, "c.rs", &same);
        let d = write(&dir, "d.rs", &same);
        let diags = run(&[&c, &d]);
        assert_eq!(diags.len(), 1, "the same rationale citing the same target is a copy");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    #[test]
    fn still_flags_duplicate_prose_differing_only_in_its_last_word_issue_8236() {
        // Over-exclusion guard: the reference test is about *references*. Ordinary
        // prose that differs only in its final word is still a copy-paste.
        let dir = tempfile::tempdir().unwrap();
        let rationale = |last: &str| {
            format!("// Ported verbatim from the upstream conformance suite so the rounding behaviour stays pinned to the specification text it came from {last}.\nexport const x = 1;\n")
        };
        let a = write(&dir, "a.ts", &rationale("originally"));
        let b = write(&dir, "b.ts", &rationale("initially"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "prose differing in one word is still a copy");
        assert!(diags[0].message.contains("Near-duplicate comment"));
    }

    // The ttf-parser sbix fixture from #8159: a column of one-word field labels.
    fn sbix_fixture(offsets: [&str; 3]) -> String {
        format!(
            "pub fn fixture() -> Vec<u32> {{\n    vec![\n        \
             0x0001, // version\n        0x0000, // flags\n        \
             0x0001, // number of strikes\n        0x000c, // strike offset for the first strike\n        \
             0x0014, // pixels per em\n        0x0048, // pixels per inch\n        \
             {}, // glyph data offset for glyph zero\n        \
             {}, // glyph data offset for glyph one\n        \
             {}, // glyph data offset for glyph two\n    ]\n}}\n",
            offsets[0], offsets[1], offsets[2]
        )
    }

    #[test]
    fn ignores_column_aligned_trailing_field_labels_issue_8159() {
        // Regression (#8159): a column of trailing labels satisfies the
        // same-column, next-row merge test as well as a prose run does, so nine
        // one-to-six-word field names were glued into one 35-word pseudo-paragraph
        // that then cleared `min_words`. No comment in either fixture is a
        // duplicate; the rule manufactured the length it flagged.
        let dir = tempfile::tempdir().unwrap();
        let a = write(&dir, "lib.rs", &sbix_fixture(["0x0010", "0x0030", "0x003a"]));
        let b = write(&dir, "other.rs", &sbix_fixture(["0x0010", "0x001a", "0x0024"]));
        assert!(run(&[&a, &b]).is_empty(), "a column of field labels is not a paragraph");

        // The same nine texts written as a standalone run still merge into one
        // block and still flag: only the trailing/standalone boundary changed, and
        // the words were always enough to clear the gate.
        let labels: String = [
            "version",
            "flags",
            "number of strikes",
            "strike offset for the first strike",
            "pixels per em",
            "pixels per inch",
            "glyph data offset for glyph zero",
            "glyph data offset for glyph one",
            "glyph data offset for glyph two",
        ]
        .iter()
        .map(|label| format!("    // {label}\n"))
        .collect();
        let c = write(&dir, "c.rs", &format!("pub fn c() {{\n{labels}}}\n"));
        let d = write(&dir, "d.rs", &format!("pub fn d() {{\n{labels}}}\n"));
        assert_eq!(run(&[&c, &d]).len(), 1, "a standalone run of the same lines still flags");
    }

    #[test]
    fn trailing_comment_does_not_merge_with_the_standalone_line_below_it() {
        // The two boundaries the merge has to respect, checked through the word
        // gate: neither run reaches `min_words` unless the trailing label is glued
        // to the standalone prose under it.
        let dir = tempfile::tempdir().unwrap();
        let block = "let x = 1; // pixels per em\n\
             // and the remaining strike offsets follow in ascending order below\n\
             // so a reader can diff two fixtures by their offsets alone here\n";
        let a = write(&dir, "a.rs", &format!("pub fn a() {{\n{block}}}\n"));
        let b = write(&dir, "b.rs", &format!("pub fn b() {{\n{block}}}\n"));
        let diags = run(&[&a, &b]);
        assert_eq!(diags.len(), 1, "the standalone run flags on its own words");
        // The reported group starts at the standalone run, not at the label above.
        assert_eq!(diags[0].line, 3);
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
