//! Look for `status: 2`, `status: 4`, `status: 5` (HTTP code-shaped
//! values) in a context that looks like a response body. The line
//! either starts with `return` or sits inside `c.json(`, `res.json(`,
//! or `Response.json(` (Hono / Express / Web).
//!
//! Skipped contexts:
//! - `new Response(body, { status: ... })` — `status` is the
//!   `ResponseInit` options bag, not a body field.
//! - Object literals carrying the RFC 7807 trio (`type` + `title`
//!   sibling fields) — Problem Details mandates `status` in the body.
//! - Object literals whose enclosing binding name contains "problem"
//!   (e.g. `const Problem = ...`, `const ApiProblem = ...`).

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

const PREFIXES: &[&str] = &["status: 2", "status: 3", "status: 4", "status: 5"];

fn line_starts_with_response_context(source: &str, offset: usize) -> bool {
    // Walk back to the previous newline.
    let prev_nl = source[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line = &source[prev_nl..offset];
    // Look at the broader scope: ~500 chars back for a return statement
    // or response method invocation.
    let mut look_start = offset.saturating_sub(500);
    while look_start > 0 && !source.is_char_boundary(look_start) {
        look_start -= 1;
    }
    let scope = &source[look_start..offset];
    let scope_signals = [
        "return ",
        "return{",
        ".json(",
        "Response.json(",
        "c.json(",
        "res.json(",
        "res.send(",
    ];
    
    line.contains("return") || scope_signals.iter().any(|s| scope.contains(s))
}

/// Find the start of the enclosing `{ ... }` literal that contains `offset`.
/// Walks backward, tracking brace depth.
fn enclosing_object_open(source: &str, offset: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = offset;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Return the slice of source containing the sibling fields of the object
/// literal enclosing `offset`. Walks back to its `{`, forward to the matching
/// `}`, returning the inner span.
fn enclosing_object_body(source: &str, offset: usize) -> Option<&str> {
    let open = enclosing_object_open(source, offset)?;
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                if depth == 0 {
                    return Some(&source[open + 1..i]);
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// RFC 7807 Problem trio: `type` + `title` as sibling fields in the same
/// object literal. Only counts matches at brace depth 0 within `object_body`
/// (i.e. direct children of the enclosing object, not keys inside a nested
/// object literal).
fn has_rfc7807_trio(object_body: &str) -> bool {
    has_field_at_depth0(object_body, "type") && has_field_at_depth0(object_body, "title")
}

/// Match a `name:` or `name :` field key that is a direct child of the
/// object body (brace depth 0). Nested objects are skipped.
fn has_field_at_depth0(body: &str, name: &str) -> bool {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
            }
            _ => {
                // Only match at depth 0 (direct siblings of status).
                if depth == 0
                    && body.get(i..).is_some_and(|s| s.starts_with(name))
                {
                    let pre_ok = i == 0
                        || !{
                            let c = bytes[i - 1];
                            c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
                        };
                    // Skip whitespace after the name, then expect a colon.
                    let mut j = i + name.len();
                    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                        j += 1;
                    }
                    if pre_ok && j < bytes.len() && bytes[j] == b':' {
                        return true;
                    }
                }
                i += 1;
            }
        }
    }
    false
}

/// Check whether the binding holding the enclosing object literal has a name
/// containing "problem" (case-insensitive). Looks ~200 chars before the open
/// brace for `const Foo = ` / `let Foo: ` / `Foo: z.object({` / `type Foo = `.
fn enclosing_binding_is_problem(source: &str, open: usize) -> bool {
    let mut look_start = open.saturating_sub(200);
    // Walk forward to a char boundary in case we landed inside a multi-byte char.
    while look_start < open && !source.is_char_boundary(look_start) {
        look_start += 1;
    }
    let pre = &source[look_start..open];
    // Look for an identifier appearing before the `{`. The most recent
    // identifier is usually the binding name (or schema field name).
    let bytes = pre.as_bytes();
    let mut end = bytes.len();
    // Skip trailing whitespace and punctuation up to the identifier.
    while end > 0 {
        let c = bytes[end - 1];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
            break;
        }
        end -= 1;
    }
    let mut start = end;
    while start > 0 {
        let c = bytes[start - 1];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
            start -= 1;
        } else {
            break;
        }
    }
    // Walk back through up to a few identifiers (z, object, Problem, etc.)
    // and check if any of them contain "problem".
    let mut cursor_end = end;
    let mut cursor_start = start;
    for _ in 0..6 {
        if cursor_start == cursor_end {
            break;
        }
        let ident = &pre[cursor_start..cursor_end];
        if ident.to_ascii_lowercase().contains("problem") {
            return true;
        }
        // Move past whitespace/punct to next identifier back.
        let mut e = cursor_start;
        while e > 0 {
            let c = bytes[e - 1];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                break;
            }
            e -= 1;
        }
        let mut s = e;
        while s > 0 {
            let c = bytes[s - 1];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                s -= 1;
            } else {
                break;
            }
        }
        if s == e {
            break;
        }
        cursor_end = e;
        cursor_start = s;
    }
    false
}

/// Detect `new Response(<body>, { status: ... })` — `status` here is the
/// `ResponseInit` options bag, not a body field. We're already inside the
/// `{ ... }` second argument; walk back from the `{` past whitespace and a
/// comma; if a `Response(` opener with `new ` keyword precedes the comma,
/// it's the init bag.
fn is_response_init_bag(source: &str, open: usize) -> bool {
    let bytes = source.as_bytes();
    let mut i = open;
    // Walk back past whitespace.
    while i > 0 {
        let c = bytes[i - 1];
        if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
            i -= 1;
        } else {
            break;
        }
    }
    // Expect a comma immediately before.
    if i == 0 || bytes[i - 1] != b',' {
        return false;
    }
    i -= 1;
    // Walk back to the matching `Response(` opener, tracking paren depth.
    let mut depth = 0i32;
    let mut j = i;
    while j > 0 {
        j -= 1;
        match bytes[j] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    // Found the `(` of `Response(`. Read the identifier
                    // before it.
                    let mut e = j;
                    while e > 0 {
                        let c = bytes[e - 1];
                        if c == b' ' || c == b'\t' {
                            e -= 1;
                        } else {
                            break;
                        }
                    }
                    let mut s = e;
                    while s > 0 {
                        let c = bytes[s - 1];
                        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                            s -= 1;
                        } else {
                            break;
                        }
                    }
                    let ident = &source[s..e];
                    if ident != "Response" {
                        return false;
                    }
                    // Look further back for `new` keyword.
                    let mut k = s;
                    while k > 0 {
                        let c = bytes[k - 1];
                        if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                            k -= 1;
                        } else {
                            break;
                        }
                    }
                    // Also require a left word boundary so that an identifier
                    // ending in "new" (e.g. `renew Response(...)`) is not
                    // mistaken for the `new` keyword.
                    return k >= 3
                        && &source[k - 3..k] == "new"
                        && (k == 3 || {
                            let c = bytes[k - 4];
                            !c.is_ascii_alphanumeric() && c != b'_' && c != b'$'
                        });
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    false
}

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for prefix in PREFIXES {
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(prefix) {
            let abs = from + rel;
            // Ensure word boundary before "status".
            let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
            let pre_ok = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
            if pre_ok && line_starts_with_response_context(source, abs) {
                // Extra: need to read 3 digits after "status: " to distinguish
                // 200 from 2 (timeout-ish). Look at ~5 chars ahead.
                let after = &source[abs + "status: ".len()..];
                let bs = after.as_bytes();
                let mut k = 0usize;
                while k < bs.len() && bs[k].is_ascii_digit() {
                    k += 1;
                }
                if k >= 3 {
                    // Skip: enclosing object literal is the `ResponseInit`
                    // bag of `new Response(body, { status: ... })`.
                    if let Some(open) = enclosing_object_open(source, abs) {
                        if is_response_init_bag(source, open) {
                            from = abs + prefix.len();
                            continue;
                        }
                        // Skip: enclosing object literal is shaped like an
                        // RFC 7807 Problem — `type` + `title` siblings.
                        if let Some(body) = enclosing_object_body(source, abs) {
                            if has_rfc7807_trio(body) {
                                from = abs + prefix.len();
                                continue;
                            }
                        }
                        // Skip: the binding name on the LHS of the enclosing
                        // schema mentions "problem".
                        if enclosing_binding_is_problem(source, open) {
                            from = abs + prefix.len();
                            continue;
                        }
                    }
                    out.push(abs);
                }
            }
            from = abs + prefix.len();
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["status:"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !looks_like_api_path(ctx.path) {
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
                    message: "HTTP status code in response body — set the response status instead and drop the field."
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

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_status_200_in_return() {
        let src = "function GET() { return { status: 200, data: 1 }; }";
        assert_eq!(run_at(src, "src/routes/x.ts").len(), 1);
    }

    #[test]
    fn flags_status_400_in_c_json() {
        let src = "app.get('/x', (c) => c.json({ status: 400, error: 'bad' }))";
        assert_eq!(run_at(src, "src/api/x.ts").len(), 1);
    }

    #[test]
    fn allows_status_outside_api_files() {
        let src = "return { status: 200, data: 1 };";
        assert!(run_at(src, "src/lib/util.ts").is_empty());
    }

    #[test]
    fn allows_status_short_value() {
        // `status: 2` alone (2-char code) probably means a state, not HTTP.
        let src = "return { status: 2, data: 1 };";
        assert!(run_at(src, "src/routes/x.ts").is_empty());
    }

    #[test]
    fn allows_rfc7807_problem_schema_with_type_title_siblings() {
        let src = r#"
            const Problem = z.object({
              type: z.string(),
              title: z.string(),
              status: 403,
              detail: z.string().optional(),
              instance: z.string().optional(),
            });
            return Problem;
        "#;
        assert!(run_at(src, "src/api/errors/index.ts").is_empty());
    }

    // Multi-byte chars (em dash in a comment) inside the object body must not
    // panic the byte-indexed field scanner.
    #[test]
    fn handles_multibyte_chars_in_object_body() {
        let src = "return {\n  type: \"notFound\",\n  // comply-ignore: api-no-status-in-body \u{2014} fixture for a Problem body.\n  status: 404,\n  title: \"Not Found\",\n};";
        let diags = run_at(src, "src/api/x.ts");
        assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    }

    #[test]
    fn allows_problem_named_binding_even_without_trio() {
        let src = r#"
            const Problem = z.object({
              status: 500,
              detail: z.string(),
            });
            return Problem;
        "#;
        assert!(run_at(src, "src/api/errors/index.ts").is_empty());
    }

    #[test]
    fn allows_to_match_object_with_problem_trio() {
        let src = r#"
            return expect(body).toMatchObject({
              type: "https://example.com/probs/forbidden",
              title: "Forbidden",
              status: 403,
            });
        "#;
        assert!(
            run_at(src, "src/api/features/auth/auth-flows.integration.test.ts").is_empty()
        );
    }

    #[test]
    fn allows_response_init_status_option() {
        let src = r#"
            export function handler() {
              return new Response(body, { status: 403 });
            }
        "#;
        assert!(run_at(src, "src/api/features/teams/extract-teams-csv.ts").is_empty());
    }

    #[test]
    fn allows_response_init_status_option_multiline() {
        let src = r#"
            return new Response(JSON.stringify(payload), {
              status: 403,
              headers: { "Content-Type": "application/problem+json" },
            });
        "#;
        assert!(run_at(src, "src/api/x.ts").is_empty());
    }

    #[test]
    fn still_flags_plain_status_in_handler_return() {
        // No type/title siblings, no "Problem" binding, not a Response init.
        let src = r#"
            app.get('/x', (c) => c.json({ status: 400, data: 1, error: 'bad' }));
        "#;
        assert_eq!(run_at(src, "src/api/x.ts").len(), 1);
    }

    #[test]
    fn still_flags_status_in_return_object_without_rfc7807_markers() {
        let src = r#"
            function handler() {
              return c.json({
                status: 500,
                data: undefined,
              });
            }
        "#;
        assert_eq!(run_at(src, "src/api/envelope.ts").len(), 1);
    }

    // Fix 1 regression: multi-byte UTF-8 within ~200 chars before status must not panic.
    #[test]
    fn no_panic_on_multibyte_utf8_before_status() {
        // The accented French comment lands within the 200-byte look-back window.
        let src = "// règle de validité\nfunction handler() { return { status: 403, error: 'bad' }; }";
        // Should not panic; may or may not flag depending on context detection.
        let _ = run_at(src, "src/api/x.ts");
    }

    // Fix 2 regression: `renew Response(...)` must not be exempted (not `new`).
    #[test]
    fn flags_status_after_renew_response_not_exempted() {
        let src = r#"
            function handler() {
              return renew Response(body, { status: 403 });
            }
        "#;
        assert_eq!(run_at(src, "src/api/x.ts").len(), 1);
    }

    // Fix 3a regression: type/title in a NESTED object must not exempt status at outer level.
    #[test]
    fn flags_status_when_type_title_only_in_nested_object() {
        let src = r#"
            return c.json({
              status: 400,
              meta: {
                type: "https://example.com/errors/bad-request",
                title: "Bad Request",
              },
            });
        "#;
        assert_eq!(run_at(src, "src/api/x.ts").len(), 1);
    }

    // Fix 3b positive: type/title as direct siblings still exempts.
    #[test]
    fn allows_rfc7807_when_type_title_are_direct_siblings() {
        let src = r#"
            return c.json({
              type: "https://example.com/errors/forbidden",
              title: "Forbidden",
              status: 403,
              detail: "You do not have access.",
            });
        "#;
        assert!(run_at(src, "src/api/x.ts").is_empty());
    }
}
