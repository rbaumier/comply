//! Two patterns flagged:
//!
//! 1. `.safeParse(...).data` — direct chained access, never branches on
//!    `.success`.
//! 2. `const r = X.safeParse(...);` followed by `r.data` access without
//!    any `r.success` check in the same window (~2KB).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

/// Match the closing paren of a `safeParse(` call starting at `open`
/// (pointing at `(`). Returns the byte offset just after `)`.
fn end_of_call(source: &[u8], open: usize) -> Option<usize> {
    if source.get(open) != Some(&b'(') {
        return None;
    }
    let mut depth = 1i32;
    let mut i = open + 1;
    while i < source.len() {
        match source[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn safe_boundary(source: &str, pos: usize) -> usize {
    let mut p = pos.min(source.len());
    while p > 0 && !source.is_char_boundary(p) {
        p -= 1;
    }
    p
}

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    let bytes = source.as_bytes();
    while from < source.len() {
        let safe_from = safe_boundary(source, from);
        let Some(rel) = source[safe_from..].find(".safeParse(") else {
            break;
        };
        let abs = safe_from + rel;
        let open = abs + ".safeParse".len();
        let Some(end) = end_of_call(bytes, open) else {
            break;
        };
        let safe_end = safe_boundary(source, end);
        if source[safe_end..].starts_with(".data") {
            out.push(abs);
            from = end;
            continue;
        }
        from = end;
    }
    let mut from = 0usize;
    while from < source.len() {
        let safe_from = safe_boundary(source, from);
        let Some(rel) = source[safe_from..].find(".safeParse(") else {
            break;
        };
        let abs = safe_from + rel;
        let preceding = &source[..abs];
        let look_start = safe_boundary(preceding, preceding.len().saturating_sub(120));
        let snippet = &preceding[look_start..];
        let mut id: Option<&str> = None;
        for keyword in ["const ", "let ", "var "] {
            if let Some(pos) = snippet.rfind(keyword) {
                let after_kw = &snippet[pos + keyword.len()..];
                let bs = after_kw.as_bytes();
                let mut k = 0usize;
                while k < bs.len()
                    && (bs[k].is_ascii_alphanumeric() || bs[k] == b'_' || bs[k] == b'$')
                {
                    k += 1;
                }
                let cand = &after_kw[..k];
                if !cand.is_empty() && after_kw[k..].trim_start().starts_with('=') {
                    id = Some(cand);
                    break;
                }
            }
        }
        let open = abs + ".safeParse".len();
        let Some(end) = end_of_call(bytes, open) else {
            break;
        };
        if let Some(name) = id {
            let win_end = safe_boundary(source, (end + 2048).min(source.len()));
            let safe_end = safe_boundary(source, end);
            let window = &source[safe_end..win_end];
            let data_pat = format!("{name}.data");
            let success_pat = format!("{name}.success");
            if let Some(data_pos) = window.find(&data_pat) {
                let before_data = &window[..data_pos];
                if !before_data.contains(&success_pat) {
                    let destructure_pat = format!("= {name};");
                    let has_destructure_check =
                        before_data.contains(&destructure_pat) && before_data.contains("success");
                    if !has_destructure_check {
                        out.push(abs);
                    }
                }
            }
        }
        from = end;
    }
    out.sort_unstable();
    out.dedup();
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains(".safeParse(") {
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
                    message: "`.safeParse(...).data` accessed without checking `.success` — \
                              validation failures are silently ignored."
                        .into(),
                    severity: Severity::Error,
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_chained_data_access() {
        let src = "const v = Schema.safeParse(input).data;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_var_data_without_success_check() {
        let src = "const r = Schema.safeParse(input);\nconst v = r.data;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_branch_on_success() {
        let src = "const r = Schema.safeParse(input);\nif (r.success) { console.log(r.data); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_destructured_with_success_check() {
        let src = "const r = Schema.safeParse(input);\nconst { data, success } = r;\nif (success) console.log(data);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_parse_method() {
        let src = "const v = Schema.parse(input).foo;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_crash_on_georgian_before_safeparse_window() {
        let src = "// ქართული ტექსტი\nconst r = Schema.safeParse(input);\nconst v = r.data;";
        assert_eq!(run(src).len(), 1);
    }
}
