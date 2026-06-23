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
        for func in BAD_FUNCS {
            if window.contains(func) {
                return Some(func.trim_end_matches('('));
            }
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
