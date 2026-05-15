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

/// Substrings that mark a schema as a response/output (not an input).
/// `Select` matches the Drizzle/zod-drizzle convention where row-shaped
/// schemas are named `*SelectSchema`.
const RESPONSE_NAME_MARKERS: &[&str] = &[
    "Response",
    "Output",
    "Return",
    "Detail",
    "Select",
];

/// Build a list of `(start, end)` byte ranges covering top-level
/// declarations of response-shaped zod schemas — `const FooResponseSchema = …`,
/// `const BarSelectSchema = …`, etc. Offenses inside these ranges are not
/// flagged: response schemas are server-controlled and don't need `.max()`.
fn response_schema_ranges(source: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let bytes = source.as_bytes();
    for (i, line_start) in source
        .lines()
        .scan(0usize, |off, line| {
            let s = *off;
            *off += line.len() + 1; // newline
            Some((line, s))
        })
        .enumerate()
    {
        let _ = i;
        let (line, off) = line_start;
        let trimmed = line.trim_start();
        let lead = line.len() - trimmed.len();
        // Look for top-level `export const Foo... = ` or `const Foo... = `
        let after_kw = trimmed
            .strip_prefix("export const ")
            .or_else(|| trimmed.strip_prefix("const "));
        let Some(rest) = after_kw else { continue };
        let Some(eq_idx) = rest.find('=') else { continue };
        let name = rest[..eq_idx].split([':', ' ']).next().unwrap_or("").trim();
        if !RESPONSE_NAME_MARKERS.iter().any(|m| name.contains(m)) {
            continue;
        }
        // Range starts at the `=`. Find the end-of-statement by tracking
        // brace / paren depth from the `=` until we hit `;` or EOL at depth 0.
        let eq_abs = off + lead + after_kw.unwrap().find('=').unwrap();
        let mut i = eq_abs + 1;
        let mut depth_paren = 0i32;
        let mut depth_brace = 0i32;
        let mut in_str: Option<u8> = None;
        while i < bytes.len() {
            let c = bytes[i];
            if let Some(q) = in_str {
                if c == b'\\' {
                    i += 2;
                    continue;
                }
                if c == q {
                    in_str = None;
                }
                i += 1;
                continue;
            }
            match c {
                b'"' | b'\'' | b'`' => in_str = Some(c),
                b'(' => depth_paren += 1,
                b')' => depth_paren -= 1,
                b'{' => depth_brace += 1,
                b'}' => depth_brace -= 1,
                b';' if depth_paren == 0 && depth_brace == 0 => {
                    i += 1;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        ranges.push((eq_abs, i));
    }
    ranges
}

fn find_offenses(source: &str) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let exclude = response_schema_ranges(source);
    for target in TARGETS {
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(target) {
            let abs = from + rel;
            let open = abs + target.len() - 1; // position of `(`
            let Some(end) = end_of_call(bytes, open) else {
                break;
            };
            let in_response = exclude.iter().any(|(s, e)| abs >= *s && abs < *e);
            if !in_response && !chain_has_max(source, end) {
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

    #[test]
    fn ignores_response_schema_by_name() {
        // Regression for #80 — response/select schemas are server-emitted,
        // not user inputs, and don't need `.max()`.
        let src = "export const OrganizationDetailSchema = z.object({\n  teams: z.array(TeamSelectSchema),\n  members: z.array(OrganizationMemberSchema),\n});";
        assert!(run_at(src, "src/api/orgs.ts").is_empty());
    }

    #[test]
    fn still_flags_input_schema_alongside_response() {
        let src = "export const CreateOrgInputSchema = z.object({ name: z.string() });\nexport const OrgResponseSchema = z.object({ teams: z.array(Team) });";
        let diags = run_at(src, "src/api/orgs.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("z.string"));
    }
}
