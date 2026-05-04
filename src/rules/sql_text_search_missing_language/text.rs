use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

// Longest-first so we never double-match a shorter suffix.
const FUNCTIONS: &[&str] = &[
    "websearch_to_tsquery(",
    "phraseto_tsquery(",
    "plainto_tsquery(",
    "to_tsvector(",
    "to_tsquery(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            let mut pos = 0usize;
            while pos < lower.len() {
                if let Some((fname, offset)) = find_next_function(&lower, pos) {
                    let arg_start = offset + fname.len();
                    if let Some(args) = extract_paren_body(&lower, arg_start)
                        && !has_language_first_arg(&args) {
                            diagnostics.push(Diagnostic {
                                path: std::sync::Arc::clone(&ctx.path_arc),
                                line: idx + 1,
                                column: 1,
                                rule_id: "sql-text-search-missing-language".into(),
                                message: "`to_tsvector`/`to_tsquery` without a language argument is not IMMUTABLE and depends on `default_text_search_config`. Pass the language explicitly, e.g. `'english'`.".into(),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
                    pos = arg_start;
                } else {
                    break;
                }
            }
        }
        diagnostics
    }
}

fn find_next_function<'a>(lower: &str, from: usize) -> Option<(&'a str, usize)> {
    let mut best: Option<(&str, usize)> = None;
    for fname in FUNCTIONS {
        if let Some(p) = lower[from..].find(fname) {
            let abs = from + p;
            match best {
                None => best = Some((fname, abs)),
                Some((_, prev_abs)) if abs < prev_abs => best = Some((fname, abs)),
                Some((prev_f, prev_abs)) if abs == prev_abs && fname.len() > prev_f.len() => {
                    best = Some((fname, abs));
                }
                _ => {}
            }
        }
    }
    best
}

/// Given a position right after `(`, return the substring up to the matching `)`.
fn extract_paren_body(lower: &str, start: usize) -> Option<String> {
    let bytes = lower.as_bytes();
    let mut depth = 1;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(lower[start..i].to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// True if the call's first argument looks like a regconfig (a single-quoted
/// string literal). The two-argument form is `to_tsvector('english', col)`.
fn has_language_first_arg(args: &str) -> bool {
    let trimmed = args.trim_start();
    if !trimmed.starts_with('\'') {
        return false;
    }
    // Must contain a comma after the closing quote (otherwise it's a single
    // quoted string arg, which means it's still the one-arg form: `to_tsquery('foo')`).
    let after = &trimmed[1..];
    if let Some(end_quote) = after.find('\'') {
        let rest = &after[end_quote + 1..];
        return rest.trim_start().starts_with(',');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_to_tsvector_single_arg() {
        assert_eq!(
            run("CREATE INDEX idx ON docs USING GIN (to_tsvector(body));").len(),
            1
        );
    }

    #[test]
    fn allows_to_tsvector_with_language() {
        assert!(
            run("CREATE INDEX idx ON docs USING GIN (to_tsvector('english', body));").is_empty()
        );
    }

    #[test]
    fn flags_to_tsquery_single_arg() {
        assert_eq!(run("SELECT to_tsquery('cats');").len(), 1);
    }

    #[test]
    fn allows_to_tsquery_with_language() {
        assert!(run("SELECT to_tsquery('english', 'cats');").is_empty());
    }

    #[test]
    fn flags_plainto_tsquery_single_arg() {
        assert_eq!(run("SELECT plainto_tsquery(query);").len(), 1);
    }
}
