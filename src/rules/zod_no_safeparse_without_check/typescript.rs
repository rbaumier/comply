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

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    let bytes = source.as_bytes();
    while let Some(rel) = source[from..].find(".safeParse(") {
        let abs = from + rel;
        let open = abs + ".safeParse".len();
        let Some(end) = end_of_call(bytes, open) else { break };
        // Direct chained `.data`?
        if source[end..].starts_with(".data") {
            out.push(abs);
            from = end;
            continue;
        }
        from = end;
    }
    // Pattern 2: `const <id> = ...safeParse(...)` followed by `<id>.data`
    // without `<id>.success` between them.
    let mut from = 0usize;
    while let Some(rel) = source[from..].find(".safeParse(") {
        let abs = from + rel;
        // Look back to find variable assignment.
        let preceding = &source[..abs];
        let look_start = preceding.len().saturating_sub(120);
        let snippet = &preceding[look_start..];
        // Look for `const <id> =` or `let <id> =` ending right before `abs`.
        let mut id: Option<&str> = None;
        for keyword in ["const ", "let ", "var "] {
            if let Some(pos) = snippet.rfind(keyword) {
                let after_kw = &snippet[pos + keyword.len()..];
                // Read identifier.
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
        let Some(end) = end_of_call(bytes, open) else { break };
        if let Some(name) = id {
            // Search a window after `end` for `<name>.data` and check
            // for `<name>.success` before the `.data` access.
            let win_end = (end + 2048).min(source.len());
            let window = &source[end..win_end];
            let data_pat = format!("{name}.data");
            let success_pat = format!("{name}.success");
            if let Some(data_pos) = window.find(&data_pat) {
                let before_data = &window[..data_pos];
                if !before_data.contains(&success_pat) {
                    // Also tolerate destructuring `const { data, success } = r;`
                    // followed by check `if (success)` / `if (!success)`.
                    let destructure_pat = format!("= {name};");
                    let has_destructure_check = before_data.contains(&destructure_pat)
                        && before_data.contains("success");
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
}
