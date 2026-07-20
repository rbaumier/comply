//! sql-no-function-on-indexed-column

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-function-on-indexed-column",
    description: "Wrapping a column in a function inside WHERE kills index sargability.",
    remediation: "Avoid `WHERE date_trunc('day', created_at) = ...` / `WHERE LOWER(email) = ...`. Store the normalized form, or add a functional index.",
    severity: Severity::Error,
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
        ],
    }
}

const BAD_FUNCS: &[&str] = &[
    "DATE_TRUNC(",
    "LOWER(",
    "UPPER(",
    "COALESCE(",
    "EXTRACT(",
    "CAST(",
    "TO_CHAR(",
];

/// Clause keywords that terminate a WHERE predicate. A banned function after
/// one of these (at paren-depth 0) sits in SELECT / GROUP BY / ORDER BY /
/// HAVING / a subquery boundary, not in the WHERE predicate, so it does not
/// defeat an index seek. Each carries surrounding spaces because the scan
/// normalizes whitespace to single spaces first.
const WHERE_TERMINATORS: &[&str] = &[
    " GROUP BY ",
    " HAVING ",
    " ORDER BY ",
    " LIMIT ",
    " OFFSET ",
    " WINDOW ",
    " UNION ",
    " INTERSECT ",
    " EXCEPT ",
    " RETURNING ",
];

/// If the SQL string applies a banned function to a column *inside a WHERE
/// predicate*, return the function name (without trailing paren). The window
/// for each WHERE is bounded at the end of its predicate, so functions in
/// SELECT / GROUP BY / ORDER BY / HAVING positions do not fire (#5752).
pub(super) fn find_bad_func_in_where(sql: &str) -> Option<&'static str> {
    // Normalize whitespace so multi-line templates expose clause keywords like
    // " GROUP BY " as contiguous tokens; pad so a terminator at the very end is
    // still bounded by a trailing space.
    let upper: String = sql.to_ascii_uppercase().split_whitespace().collect::<Vec<_>>().join(" ");
    let padded = format!(" {upper} ");

    let mut search_from = 0;
    while let Some(rel) = padded[search_from..].find("WHERE ") {
        let where_start = search_from + rel + "WHERE ".len();
        let rest = &padded[where_start..];
        let window_end = where_clause_end(rest);
        let window = &rest[..window_end];
        if let Some(func) = bad_func_wrapping_column(window) {
            return Some(func);
        }
        search_from = where_start + window_end;
    }
    None
}

/// Byte length of the WHERE-predicate window at the start of `rest` (uppercased,
/// whitespace-normalized). The window ends at the first clause terminator at
/// paren-depth 0, or at the first `)` that closes an *enclosing* paren (a CTE /
/// subquery wrapper) — whichever comes first. A balanced `(...)` inside the
/// predicate (`WHERE (a OR b) AND LOWER(x) = …`) does not end it, so a banned
/// function after a parenthesized condition still fires.
fn where_clause_end(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
            }
            b' ' if depth == 0 => {
                let tail = &rest[i..];
                if WHERE_TERMINATORS.iter().any(|t| tail.starts_with(t)) {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    rest.len()
}

/// SQL keywords that can appear as top-level tokens inside a banned function's
/// argument list but never denote a column: clause words, boolean/null
/// literals, and the `AS` / `FROM` markers used to locate CAST target types and
/// EXTRACT fields.
const ARG_KEYWORDS: &[&str] = &[
    "AS", "FROM", "SELECT", "DISTINCT", "NULL", "TRUE", "FALSE", "AND", "OR", "NOT", "IS",
];

/// Within a WHERE-predicate `window` (uppercased, whitespace-normalized), return
/// the first banned function that actually *wraps a column* — the shape that
/// defeats an index seek (`LOWER(email) = …`, `COALESCE(deleted_at, 'inf') > …`).
/// A banned function whose arguments are only a subquery, a bind parameter, or
/// literals (`col <= COALESCE((SELECT …), 0)`) leaves the indexed column bare on
/// the sargable side and is skipped.
fn bad_func_wrapping_column(window: &str) -> Option<&'static str> {
    for func in BAD_FUNCS {
        let mut from = 0;
        while let Some(rel) = window[from..].find(func) {
            let open = from + rel + func.len() - 1; // index of the '('
            if arg_list_has_column(&window[open..]) {
                return Some(func.trim_end_matches('('));
            }
            from = open + 1;
        }
    }
    None
}

/// Given `s` starting at a function's opening `(`, whether its argument list
/// contains a bare column reference — an unqualified or `table.column`
/// identifier that the function actually wraps. A column wrapped in a grouping /
/// arithmetic sub-expression still counts (`EXTRACT(EPOCH FROM (now() -
/// created_at))`). The following do not: identifiers inside a nested subquery
/// (`(SELECT …)` / `(WITH …)` / `(VALUES …)`), which belong to another scope;
/// bind parameters (`$1`, `:name`); string literals; numbers; keywords; a CAST
/// target type (the token after `AS`); an EXTRACT field (the token before
/// `FROM`); and nested function-call names (an identifier followed by `(`).
fn arg_list_has_column(s: &str) -> bool {
    let bytes = s.as_bytes();
    // (token, is_call) for each identifier in the argument list, in order.
    let mut tokens: Vec<(&str, bool)> = Vec::new();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => {
                if depth >= 1 && opens_subquery(bytes, i) {
                    i = skip_parens(bytes, i); // a subquery wraps no outer column
                } else {
                    depth += 1;
                    i += 1;
                }
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    break; // closed the function's own argument list
                }
            }
            b'\'' => i = skip_string_literal(bytes, i),
            b'$' | b':' => {
                // Bind parameter (`$1`, `$name`, `:name`) or `::type` cast: consume
                // the marker and the following identifier so it is not read as a
                // column.
                i += 1;
                while i < bytes.len() && is_ident_cont(bytes[i]) {
                    i += 1;
                }
            }
            b if is_ident_start(b) => {
                let start = i;
                i += 1;
                while i < bytes.len() && is_ident_cont(bytes[i]) {
                    i += 1;
                }
                let mut k = i;
                while k < bytes.len() && bytes[k] == b' ' {
                    k += 1;
                }
                let is_call = k < bytes.len() && bytes[k] == b'(';
                tokens.push((&s[start..i], is_call));
            }
            _ => i += 1,
        }
    }
    (0..tokens.len()).any(|idx| is_bare_column(&tokens, idx))
}

/// Whether the token at position `idx` is a bare column reference: not a
/// keyword, not a function-call name, not a CAST target type (the token after
/// `AS`), and not an EXTRACT field (the token before `FROM`).
fn is_bare_column(tokens: &[(&str, bool)], idx: usize) -> bool {
    let (tok, is_call) = tokens[idx];
    if is_call || ARG_KEYWORDS.contains(&tok) {
        return false;
    }
    if idx > 0 && tokens[idx - 1].0 == "AS" {
        return false; // CAST target type
    }
    if idx + 1 < tokens.len() && tokens[idx + 1].0 == "FROM" {
        return false; // EXTRACT field
    }
    true
}

/// Whether the parenthesized group opening at `bytes[open] == '('` is a subquery
/// — its first token is `SELECT`, `WITH`, or `VALUES` — rather than a grouping /
/// arithmetic sub-expression.
fn opens_subquery(bytes: &[u8], open: usize) -> bool {
    let mut j = open + 1;
    while j < bytes.len() && bytes[j] == b' ' {
        j += 1;
    }
    let start = j;
    while j < bytes.len() && is_ident_cont(bytes[j]) {
        j += 1;
    }
    matches!(&bytes[start..j], b"SELECT" | b"WITH" | b"VALUES")
}

/// Advance past a balanced parenthesized group starting at `bytes[open] == '('`,
/// honoring string literals so a `)` inside a literal does not close it. Returns
/// the index just past the matching `)` (or the end if unbalanced).
fn skip_parens(bytes: &[u8], open: usize) -> usize {
    let mut depth: i32 = 0;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            b'\'' => {
                i = skip_string_literal(bytes, i);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    i
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

/// Advance past a single-quoted SQL string literal starting at the opening quote
/// `bytes[i] == '\''`, treating `''` as an escaped quote. Returns the index just
/// past the closing quote (or the end if unterminated).
fn skip_string_literal(bytes: &[u8], mut i: usize) -> usize {
    i += 1; // opening quote
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                i += 2; // escaped quote
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    i
}
