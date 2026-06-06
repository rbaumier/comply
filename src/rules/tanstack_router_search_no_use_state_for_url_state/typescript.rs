//! Heuristic: in a file that imports `@tanstack/react-router`, find every
//! `useState` destructuring whose first binding name matches one of the
//! URL-shaped names.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const URL_NAMES: &[&str] = &["filter", "page", "sort", "tab", "search", "query"];

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn imports_tanstack_router(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@tanstack/react-router")
}

fn matches_url_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    URL_NAMES
        .iter()
        .any(|n| lower == *n || lower.starts_with(n) || lower.ends_with(n))
}

/// Find offsets for `const [<urlName>, set...] = useState(...)`.
fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("useState") {
        let us_abs = from + rel;
        // Word boundary
        let pre = source.as_bytes().get(us_abs.saturating_sub(1)).copied();
        let post = source.as_bytes().get(us_abs + "useState".len()).copied();
        let pre_ok = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
        let post_ok = post == Some(b'(') || post == Some(b'<');
        if !pre_ok || !post_ok {
            from = us_abs + 1;
            continue;
        }
        // Look back for `const [<name>` or `let [<name>`.
        let preceding = &source[..us_abs];
        // Find the nearest preceding `[` within ~200 chars.
        let mut look_start = preceding.len().saturating_sub(200);
        while look_start > 0 && !preceding.is_char_boundary(look_start) {
            look_start -= 1;
        }
        let snippet = &preceding[look_start..];
        if let Some(bracket_pos) = snippet.rfind('[') {
            let after_bracket = &snippet[bracket_pos + 1..];
            // Read identifier.
            let mut k = 0usize;
            let bytes = after_bracket.as_bytes();
            // Skip whitespace.
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            let ident_start = k;
            while k < bytes.len()
                && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_' || bytes[k] == b'$')
            {
                k += 1;
            }
            if k > ident_start {
                let name = &after_bracket[ident_start..k];
                if matches_url_name(name) {
                    out.push(us_abs);
                }
            }
        }
        from = us_abs + 1;
    }
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !imports_tanstack_router(ctx.source) {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`useState` for URL-shaped state — use TanStack Router \
                              `Route.useSearch()` so reloads and shares preserve it."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_use_state_for_filter() {
        let src = "import { Route } from '@tanstack/react-router';\nconst [filter, setFilter] = useState('');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_state_for_page() {
        let src = "import x from '@tanstack/react-router';\nconst [page, setPage] = useState(1);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_state_for_unrelated_var() {
        let src = "import x from '@tanstack/react-router';\nconst [count, setCount] = useState(0);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_files_without_router_import() {
        let src = "const [filter, setFilter] = useState('');";
        assert!(run(src).is_empty());
    }
}
