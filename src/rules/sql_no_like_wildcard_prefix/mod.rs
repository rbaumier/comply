//! sql-no-like-wildcard-prefix

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod text;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-like-wildcard-prefix",
    description: "`LIKE '%...'` prevents index usage — use full-text search instead.",
    remediation: "Replace `LIKE '%term%'` with a TSVECTOR + GIN index and `@@` operator. Leading wildcards force a sequential scan on every row.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_drizzle::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
            (Language::Sql, Backend::Text(Box::new(text::Check))),
        ],
    }
}

/// True if `text` contains a leading-wildcard `LIKE '%...` that sits in a
/// **filter** clause — `WHERE` / `HAVING` / `JOIN ... ON`, including the
/// `AND` / `OR` predicate chains beneath them.
///
/// A leading wildcard only defeats an index when it prunes rows. The same
/// `LIKE '%...'` in the `SELECT` projection list is a per-row computed column,
/// not a scan-forcing predicate, so it is exempt. Matches single- and
/// double-quote patterns, case-insensitive on the keyword. Introspection over
/// Postgres system catalogs (`pg_catalog.*` / `information_schema.*`) is exempt
/// entirely: those tables are tiny and unindexed, so a leading wildcard costs
/// nothing.
///
/// When an occurrence's clause position cannot be determined, it is treated as
/// a non-filter (not flagged): the rule is performance advice, so a missed
/// filter `LIKE` is preferable to a false positive on a projection column.
pub(super) fn has_filter_leading_wildcard_like(text: &str) -> bool {
    if crate::rules::sql_helpers::targets_system_catalog(text) {
        return false;
    }
    filter_occurrences(text.as_bytes()).next().is_some()
}

/// Byte offset of every filter-position leading-wildcard `LIKE` keyword in
/// `text`. Used by the line-agnostic text backend, which scans the whole source
/// so it retains full clause context (multi-line predicates included).
pub(super) fn filter_leading_wildcard_like_offsets(text: &str) -> Vec<usize> {
    if crate::rules::sql_helpers::targets_system_catalog(text) {
        return Vec::new();
    }
    filter_occurrences(text.as_bytes()).collect()
}

/// Iterator over the keyword-start offset of each leading-wildcard `LIKE` in
/// `bytes` that sits in a filter clause.
fn filter_occurrences(bytes: &[u8]) -> impl Iterator<Item = usize> + '_ {
    (0..bytes.len()).filter(move |&i| {
        leading_wildcard_like_at(bytes, i).is_some_and(|quote_pos| {
            occurrence_is_filter(bytes, i, quote_pos)
        })
    })
}

/// If a leading-wildcard `LIKE '%` / `LIKE "%` starts at `i` (case-insensitive
/// keyword, word-bounded), returns the offset of the opening quote.
fn leading_wildcard_like_at(bytes: &[u8], i: usize) -> Option<usize> {
    if i + 4 > bytes.len() || !bytes[i..i + 4].eq_ignore_ascii_case(b"like") {
        return None;
    }
    if i > 0 && is_word_byte(bytes[i - 1]) {
        return None; // part of a larger identifier, e.g. `unlike`
    }
    let after = i + 4;
    if after < bytes.len() && is_word_byte(bytes[after]) {
        return None; // e.g. `likeness`
    }
    let mut j = after;
    while j < bytes.len() && (bytes[j].is_ascii_whitespace() || bytes[j] == b'\\') {
        j += 1;
    }
    if j + 1 < bytes.len() && (bytes[j] == b'\'' || bytes[j] == b'"') && bytes[j + 1] == b'%' {
        Some(j)
    } else {
        None
    }
}

/// Whether the leading-wildcard `LIKE` whose keyword starts at `like_start` and
/// whose pattern string opens at `quote_pos` is a row-pruning filter predicate.
fn occurrence_is_filter(bytes: &[u8], like_start: usize, quote_pos: usize) -> bool {
    // `LIKE '%...' AS alias` can only be a computed projection column — an
    // aliased predicate is a syntax error in a filter clause.
    if let Some(after) = string_literal_end(bytes, quote_pos) {
        if is_as_alias(bytes, after) {
            return false;
        }
    }
    matches!(nearest_preceding_clause(bytes, like_start), Some(Clause::Filter))
}

#[derive(Clone, Copy)]
enum Clause {
    Filter,
    NonFilter,
}

/// Classifies the clause governing `like_start` by the nearest preceding
/// word-bounded clause keyword at the same parenthesis depth. Keywords inside a
/// nested subquery that closes before `like_start` (deeper depth) are skipped,
/// so `WHERE id IN (SELECT ... FROM u) AND x LIKE '%y%'` still resolves to the
/// outer `WHERE`. `None` when no keyword is found (unparseable).
fn nearest_preceding_clause(bytes: &[u8], like_start: usize) -> Option<Clause> {
    const KEYWORDS: &[(&[u8], Clause)] = &[
        (b"where", Clause::Filter),
        (b"having", Clause::Filter),
        (b"on", Clause::Filter),
        (b"select", Clause::NonFilter),
        (b"from", Clause::NonFilter),
        (b"group", Clause::NonFilter),
        (b"order", Clause::NonFilter),
    ];
    let mut depth: i32 = 0;
    let mut pos = like_start;
    while pos > 0 {
        pos -= 1;
        match bytes[pos] {
            b')' => depth += 1, // entering a group that closed left of `like_start`
            b'(' => depth -= 1,
            _ if depth > 0 => {} // inside a nested subquery — not governing
            _ => {
                for &(kw, clause) in KEYWORDS {
                    let end = pos + kw.len();
                    if end <= like_start
                        && bytes[pos..end].eq_ignore_ascii_case(kw)
                        && !(pos > 0 && is_word_byte(bytes[pos - 1]))
                        && !(end < bytes.len() && is_word_byte(bytes[end]))
                    {
                        return Some(clause);
                    }
                }
            }
        }
    }
    None
}

/// True if `AS <alias>` begins at `pos` (skipping leading whitespace and
/// line-continuation backslashes) — the signature of a computed column.
fn is_as_alias(bytes: &[u8], pos: usize) -> bool {
    let mut k = pos;
    while k < bytes.len() && (bytes[k].is_ascii_whitespace() || bytes[k] == b'\\') {
        k += 1;
    }
    if k + 2 > bytes.len() || !bytes[k..k + 2].eq_ignore_ascii_case(b"as") {
        return false;
    }
    let after = k + 2;
    if after < bytes.len() && is_word_byte(bytes[after]) {
        return false; // e.g. `ascending`
    }
    let mut m = after;
    while m < bytes.len() && (bytes[m].is_ascii_whitespace() || bytes[m] == b'\\') {
        m += 1;
    }
    m < bytes.len() && (bytes[m].is_ascii_alphabetic() || bytes[m] == b'_' || bytes[m] == b'"')
}

/// Offset just past the closing quote of the string literal opened at
/// `quote_pos`, or `None` if it is unterminated.
fn string_literal_end(bytes: &[u8], quote_pos: usize) -> Option<usize> {
    let quote = bytes[quote_pos];
    let mut k = quote_pos + 1;
    while k < bytes.len() {
        if bytes[k] == quote {
            return Some(k + 1);
        }
        k += 1;
    }
    None
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
