//! graphql-require-id-field — for each object selection that has 2+ scalar
//! fields, ensure `id` is one of them. Only operations are inspected.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !looks_like_operation(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let blocks = collect_blocks(ctx.source);
        for block in blocks {
            // Skip the outermost operation block — we want object selections,
            // not the root.
            if block.is_root {
                continue;
            }
            let fields = parse_field_names(&block.body);
            if fields.len() < 2 {
                continue;
            }
            if !fields.iter().any(|f| f == "id" || f == "_id") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: block.line,
                    column: 1,
                    rule_id: "graphql-require-id-field".into(),
                    message: format!(
                        "Selection on `{}` has {} fields but no `id` — normalized caches need `id` to dedupe entities.",
                        block.parent, fields.len()
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn looks_like_operation(source: &str) -> bool {
    for raw in source.lines() {
        let line = strip_comment(raw).trim_start();
        if line.starts_with("query") || line.starts_with("mutation") || line.starts_with("subscription") {
            return true;
        }
    }
    false
}

#[derive(Debug)]
struct Block {
    parent: String,
    body: String,
    line: usize,
    is_root: bool,
}

/// Walk the source, find every `name(...) {` or `name {` opening, and capture
/// the matching `{...}` body. The parent token immediately before `{` (with
/// any args stripped) is recorded so diagnostics can name it.
fn collect_blocks(source: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            // Look back to find the parent name.
            let head = &source[..i];
            let parent = parent_name(head);
            let is_root = depth == 0 && is_operation_root(head);
            // Find matching close.
            let mut d = 1;
            let mut j = i + 1;
            while j < bytes.len() && d > 0 {
                match bytes[j] {
                    b'{' => d += 1,
                    b'}' => d -= 1,
                    _ => {}
                }
                if d == 0 {
                    break;
                }
                j += 1;
            }
            let body = source[i + 1..j.min(source.len())].to_string();
            let line = source[..i].matches('\n').count() + 1;
            out.push(Block { parent, body, line, is_root });
            depth += 1;
            i += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn parent_name(head: &str) -> String {
    // Strip a trailing argument list `(...)` if present.
    let trimmed = head.trim_end();
    let trimmed = if trimmed.ends_with(')') {
        let mut depth = 0i32;
        let bytes = trimmed.as_bytes();
        let mut cut = trimmed.len();
        for (i, &b) in bytes.iter().enumerate().rev() {
            match b {
                b')' => depth += 1,
                b'(' => {
                    depth -= 1;
                    if depth == 0 {
                        cut = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        &trimmed[..cut]
    } else {
        trimmed
    };
    let trimmed = trimmed.trim_end();
    // Take the last identifier token.
    let last = trimmed
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|s| !s.is_empty())
        .unwrap_or("");
    last.to_string()
}

fn is_operation_root(head: &str) -> bool {
    // The most recent keyword before this `{` is query/mutation/subscription
    // and there is no other `{` between them.
    let last_brace = head.rfind('{');
    let scan = match last_brace {
        Some(idx) => &head[idx + 1..],
        None => head,
    };
    let scan_l = scan.to_string();
    for kw in ["query", "mutation", "subscription"] {
        if scan_l.contains(kw) {
            return true;
        }
    }
    // Bare `{ ... }` shorthand (anonymous query) is also a root.
    scan.trim().is_empty()
}

/// Extract top-level field names from a selection-set body. Skips fragment
/// spreads, inline fragments, and nested selection bodies.
fn parse_field_names(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut i = 0;
    let bytes = body.as_bytes();
    let mut current = String::new();
    let mut at_token_start = true;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '#' {
            // Skip to end of line.
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            at_token_start = true;
            continue;
        }
        if c == '{' {
            depth += 1;
            i += 1;
            continue;
        }
        if c == '}' {
            depth -= 1;
            i += 1;
            continue;
        }
        if c == '(' {
            // Skip argument list.
            let mut d = 1;
            i += 1;
            while i < bytes.len() && d > 0 {
                match bytes[i] {
                    b'(' => d += 1,
                    b')' => d -= 1,
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        if depth == 0 {
            if c.is_whitespace() || c == ',' {
                at_token_start = true;
                i += 1;
                continue;
            }
            if c == ':' {
                // Alias — we already captured the alias as the field name; the
                // real field name follows. Reset so we keep the alias as the
                // identifier (which is what callers see in JSON).
                current.clear();
                at_token_start = true;
                i += 1;
                continue;
            }
            if at_token_start && (c.is_alphabetic() || c == '_') {
                let start = i;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch.is_alphanumeric() || ch == '_' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                current = body[start..i].to_string();
                // Skip `...Frag` and `...on Type` — those are fragments.
                if current == "on" {
                    current.clear();
                    continue;
                }
                // Look ahead — if the next non-space char is `:`, this is an
                // alias; consume it and continue to grab the real name.
                let mut k = i;
                while k < bytes.len() && (bytes[k] as char).is_whitespace() {
                    k += 1;
                }
                if k < bytes.len() && bytes[k] == b':' {
                    i = k + 1;
                    at_token_start = true;
                    // Keep alias as the field identifier — it's what the
                    // server returns in the response.
                    out.push(std::mem::take(&mut current));
                    continue;
                }
                out.push(std::mem::take(&mut current));
                at_token_start = false;
                continue;
            }
            if c == '.' {
                // Fragment spread `...Name` — skip the dots and the name.
                while i < bytes.len() && bytes[i] == b'.' {
                    i += 1;
                }
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch.is_alphanumeric() || ch == '_' || ch == ' ' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                at_token_start = true;
                continue;
            }
            i += 1;
            at_token_start = false;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("op.graphql"), source))
    }

    #[test]
    fn flags_selection_without_id() {
        let src = "query GetUser($id: ID!) { user(id: $id) { name email } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_nested_selection_without_id() {
        let src = "query Q { me { posts { title body } } }";
        // `posts` selection has 2 fields, no id.
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_selection_with_id() {
        let src = "query GetUser($id: ID!) { user(id: $id) { id name email } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_field_selection() {
        // Single field — likely a scalar projection, not entity-shaped.
        let src = "query Count { stats { total } }";
        assert!(run(src).is_empty());
    }
}
