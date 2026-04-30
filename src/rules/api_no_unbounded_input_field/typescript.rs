//! Heuristic: in route/api files, find zod field declarations
//! `z.string()`, `z.number()`, `z.array(...)` and look for a `.max(`
//! call somewhere in the same chain (until the next `,` or line break
//! at brace depth 0).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ROUTE_HINTS: &[&str] = &["route", "api", "handler", "controller", "endpoint"];

fn looks_like_api_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    ROUTE_HINTS.iter().any(|h| s.contains(h))
}

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

const TARGETS: &[&str] = &["z.string(", "z.number(", "z.array("];

/// Find the byte offset just after the matching `)` of a call whose `(`
/// is at `open_offset`.
fn end_of_call(bytes: &[u8], open_offset: usize) -> Option<usize> {
    if bytes.get(open_offset) != Some(&b'(') {
        return None;
    }
    let mut depth = 1i32;
    let mut i = open_offset + 1;
    while i < bytes.len() {
        match bytes[i] {
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

/// True when the chain starting at `chain_start` includes a `.max(` call
/// before it terminates (next `,` or `}` at brace depth 0, ignoring
/// content inside parens).
fn chain_has_max(source: &str, chain_start: usize) -> bool {
    let bytes = source.as_bytes();
    let mut i = chain_start;
    let mut depth_paren = 0i32;
    let mut depth_brace = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth_paren += 1,
            b')' => {
                if depth_paren == 0 {
                    break;
                }
                depth_paren -= 1;
            }
            b'{' => depth_brace += 1,
            b'}' => {
                if depth_brace == 0 {
                    break;
                }
                depth_brace -= 1;
            }
            b',' if depth_paren == 0 && depth_brace == 0 => break,
            _ => {}
        }
        if depth_paren == 0
            && depth_brace == 0
            && i + 4 < bytes.len()
            && &bytes[i..i + 5] == b".max("
        {
            return true;
        }
        i += 1;
    }
    false
}

fn find_offenses(source: &str) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    for target in TARGETS {
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(target) {
            let abs = from + rel;
            let open = abs + target.len() - 1; // position of `(`
            let Some(end) = end_of_call(bytes, open) else {
                break;
            };
            if !chain_has_max(source, end) {
                out.push((abs, *target));
            }
            from = end;
        }
    }
    out
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string(", "z.number(", "z.array("])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !looks_like_api_path(ctx.path) {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|(offset, target)| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                let kind = target.trim_end_matches('(');
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{kind}` has no `.max(N)` — unbounded API input is a resource-exhaustion vector."
                    ),
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

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_unbounded_string() {
        let src = "const Body = z.object({ name: z.string() });";
        assert_eq!(run_at(src, "src/routes/x.ts").len(), 1);
    }

    #[test]
    fn flags_unbounded_array() {
        let src = "const Body = z.object({ tags: z.array(z.string().max(20)) });";
        assert_eq!(run_at(src, "src/api/y.ts").len(), 1);
    }

    #[test]
    fn allows_string_with_max() {
        let src = "const Body = z.object({ name: z.string().max(100) });";
        assert!(run_at(src, "src/routes/x.ts").is_empty());
    }

    #[test]
    fn allows_chain_min_then_max() {
        let src = "const Body = z.object({ n: z.number().min(0).max(99) });";
        assert!(run_at(src, "src/routes/x.ts").is_empty());
    }

    #[test]
    fn ignores_non_api_files() {
        let src = "const X = z.object({ name: z.string() });";
        assert!(run_at(src, "src/lib/util.ts").is_empty());
    }
}
