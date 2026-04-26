//! vue-prefer-computed text backend.
//!
//! Detects `watch(src, (v) => { target.value = <expr> })` where the body
//! contains a single assignment to `something.value`. That pattern is a
//! derived value masquerading as a side effect — `computed()` is lazier,
//! cached, and can't desync from its source.
//!
//! Heuristic (text-based — Vue SFCs skip tree-sitter):
//! 1. Find a line starting a `watch(` or `watchEffect(` call.
//! 2. Collect the callback body by tracking brace depth until it closes.
//! 3. If the body has exactly one non-trivial statement and it is a bare
//!    `<ident>.value = <expr>` assignment, flag the watch.
//!
//! We deliberately keep the detector narrow: multi-statement bodies, calls
//! that produce side effects (console, fetch, emit, push, ...), and
//! conditional assignments are left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("watch(") && !src.contains("watchEffect(") {
            return Vec::new();
        }

        let lines: Vec<&str> = src.lines().collect();
        let mut diags = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            // Only consider lines that start a watch() call.
            let is_watch = trimmed.starts_with("watch(") || trimmed.starts_with("watchEffect(");
            if !is_watch {
                continue;
            }

            // Collect the watch statement body by following brace depth until
            // the outer `watch(...)` parenthesis closes.
            let Some((body, _)) = extract_watch_callback_body(&lines, i) else {
                continue;
            };

            if body_is_single_value_assignment(&body) {
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "This watcher only writes to a `.value` — use `computed()` \
                              for a lazy, cached derived value that can't desync."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diags
    }
}

/// Extract the body of the first `{ ... }` block inside the watch/watchEffect
/// call that starts on `lines[start]`. Returns the body text and the line
/// index of the closing brace.
fn extract_watch_callback_body(lines: &[&str], start: usize) -> Option<(String, usize)> {
    // Find the first `{` after the watch identifier — this is the start of the
    // callback body. We skip braces that appear inside the arg list but we
    // need to handle `watch(() => src, () => { ... })` too, so the *last*
    // `{` before the matching `)` is not trivial. We take the first `{` that
    // introduces a block with a depth count greater than zero.
    let mut depth: i32 = 0;
    let mut paren: i32 = 0;
    let mut in_body = false;
    let mut body = String::new();
    let mut body_start_paren: i32 = 0;

    for (j, line) in lines.iter().enumerate().skip(start) {
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            // Skip line comments.
            if c == '/' && chars.peek() == Some(&'/') {
                break;
            }
            if in_body {
                body.push(c);
            }
            match c {
                '(' => paren += 1,
                ')' => {
                    paren -= 1;
                    if paren < body_start_paren {
                        return None;
                    }
                }
                '{' => {
                    if !in_body && paren >= 1 {
                        in_body = true;
                        body_start_paren = paren;
                        body.clear();
                        continue;
                    }
                    if in_body {
                        depth += 1;
                    }
                }
                '}' => {
                    if in_body {
                        if depth == 0 {
                            // Strip the closing brace we just pushed.
                            body.pop();
                            return Some((body, j));
                        }
                        depth -= 1;
                    }
                }
                _ => {}
            }
        }
        if in_body {
            body.push('\n');
        }
    }
    None
}

/// Returns true when `body` is a single bare `<ident>.value = <expr>`
/// assignment. Whitespace, empty lines, and trailing semicolons are ignored.
fn body_is_single_value_assignment(body: &str) -> bool {
    let mut non_empty: Vec<&str> = body
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    if non_empty.len() != 1 {
        return false;
    }
    let stmt = non_empty.pop().unwrap().trim_end_matches(';').trim();

    // Require `<ident>.value = ...`, not `+=`, `-=`, etc.
    let Some(eq) = stmt.find('=') else {
        return false;
    };
    // Reject `==`, `!=`, `>=`, `<=`, `=>`.
    let before = &stmt[..eq];
    let after = &stmt[eq + 1..];
    if before.ends_with('!')
        || before.ends_with('<')
        || before.ends_with('>')
        || before.ends_with('+')
        || before.ends_with('-')
        || before.ends_with('*')
        || before.ends_with('/')
        || before.ends_with('%')
        || after.starts_with('=')
        || after.starts_with('>')
    {
        return false;
    }

    let lhs = before.trim();
    // Must be `<ident>.value` — single token, no spaces, no bracket access.
    if !lhs.ends_with(".value") {
        return false;
    }
    let ident = &lhs[..lhs.len() - ".value".len()];
    if ident.is_empty() || ident.contains(' ') || ident.contains('[') || ident.contains('(') {
        return false;
    }
    // Reject member chains like `a.b.value` — computed is still a fine
    // suggestion but the pattern then usually feeds into a reactive object,
    // not a ref. Keep the heuristic narrow to avoid false positives.
    if ident.contains('.') {
        return false;
    }
    // Identifier must be a valid JS identifier.
    if !ident
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    {
        return false;
    }
    // RHS must be non-empty.
    !after.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }

    #[test]
    fn flags_watch_that_mirrors_into_ref() {
        let src = "watch(count, (v) => {\n  doubled.value = v * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_watch_one_liner_assignment() {
        let src = "watch(source, () => { target.value = source.value + 1 })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_watch_with_side_effect() {
        let src = "watch(count, (v) => {\n  console.log(v)\n  total.value = v\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_watch_with_conditional() {
        let src = "watch(count, (v) => {\n  if (v > 0) total.value = v\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_computed() {
        let src = "const doubled = computed(() => count.value * 2)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_watch_assigning_to_reactive_object() {
        let src = "watch(count, (v) => { state.count = v })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_comment_lines() {
        let src = "// watch(count, () => { x.value = 1 })";
        assert!(run(src).is_empty());
    }
}
