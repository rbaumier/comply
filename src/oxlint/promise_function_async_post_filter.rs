//! Post-filter for `promise-function-async` false positives.
//!
//! Two categories of false positives are suppressed:
//!
//! **1. Explicit non-Promise return type** (`Effect.Effect<…>`, etc.)
//! `promise-function-async` mandates the `async` keyword on Promise-returning
//! functions. In an effect-ts codebase, functions return `Effect.Effect<…>`,
//! which is *not* a Promise — making them `async` would wrap the Effect in a
//! Promise and break the program. When a function carries an explicit return
//! type annotation that does not mention `Promise`/`PromiseLike`, the
//! diagnostic is dropped.
//!
//! **2. Concise pass-through arrow callbacks** (`(api) => api.get()`)
//! Arrow functions with no explicit return type annotation and a concise body
//! (no `{`) forward an already-pending Promise to a caller that handles it.
//! Adding `async` wraps the Promise in an extra microtask
//! (`async () => p` ≡ `Promise.resolve(p)`) with no semantic benefit.
//! Single-`return`-statement block arrows with no `await` are also exempt.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "promise-function-async" {
            return true;
        }
        let entry = file_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !returns_explicit_non_promise(src, d.line, d.column)
            && !is_passthrough_arrow(src, d.line, d.column)
    });
}

/// True when the function at the diagnostic location carries an explicit return
/// type annotation that does not mention `Promise` (e.g. `Effect.Effect<…>`).
fn returns_explicit_non_promise(src: &str, line: usize, col: usize) -> bool {
    let Some(offset) = byte_offset(src, line, col) else {
        return false;
    };
    if !src.is_char_boundary(offset) {
        return false;
    }
    let Some(ret) = return_type_annotation(&src[offset..]) else {
        return false;
    };
    !ret.contains("Promise")
}

/// 1-based (line, column) → byte offset into `src`. `column` is treated as a
/// byte offset within its line and clamped to the line length.
fn byte_offset(src: &str, line: usize, col: usize) -> Option<usize> {
    if line == 0 || col == 0 {
        return None;
    }
    let mut offset = 0usize;
    for (idx, l) in src.lines().enumerate() {
        if idx + 1 == line {
            return Some(offset + (col - 1).min(l.len()));
        }
        offset += l.len() + 1; // +1 for the stripped '\n'
    }
    None
}

/// Given source starting at (or before) a function, return its explicit return
/// type annotation text — the `…` in `(params): … {` / `(params): … =>`. The
/// scan balances the parameter-list parens, then captures the type up to the
/// body `{`, the arrow `=>`, or a `;`, tracking `<>`/`()` depth so generics and
/// inline function types don't terminate the capture early.
fn return_type_annotation(after: &str) -> Option<String> {
    let bytes = after.as_bytes();
    let open = after.find('(')?;
    let mut depth = 0i32;
    let mut i = open;
    let mut close = None;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let close = close?;
    let mut j = close + 1;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    if bytes.get(j) != Some(&b':') {
        return None;
    }
    j += 1;
    let start = j;
    let (mut angle, mut paren) = (0i32, 0i32);
    while j < bytes.len() {
        match bytes[j] {
            b'<' => angle += 1,
            b'>' if angle > 0 => angle -= 1,
            b'(' => paren += 1,
            b')' if paren > 0 => paren -= 1,
            b'{' | b';' if angle == 0 && paren == 0 => break,
            b'=' if angle == 0 && paren == 0 && bytes.get(j + 1) == Some(&b'>') => break,
            _ => {}
        }
        j += 1;
    }
    Some(after[start..j].trim().to_string())
}

/// True when the arrow function at the diagnostic location is a concise
/// pass-through with no explicit return type annotation. Two shapes qualify:
/// - Concise body: `(params) => expr` — no `await` is possible.
/// - Single-return block: `(params) => { return expr; }` with no `await`.
///
/// In both cases adding `async` only wraps the already-pending Promise in
/// `Promise.resolve(p)`, which is semantically equivalent.
fn is_passthrough_arrow(src: &str, line: usize, col: usize) -> bool {
    let Some(offset) = byte_offset(src, line, col) else {
        return false;
    };
    if !src.is_char_boundary(offset) {
        return false;
    }
    let after = &src[offset..];
    let bytes = after.as_bytes();

    let Some(open) = after.find('(') else {
        return false;
    };

    // Balance the parameter-list parens to find the closing ')'
    let mut depth = 0i32;
    let mut i = open;
    let mut close = None;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let Some(close) = close else {
        return false;
    };

    let mut j = close + 1;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }

    // Explicit return type annotation — let returns_explicit_non_promise handle it
    if bytes.get(j) == Some(&b':') {
        return false;
    }

    // Must be an arrow function
    if bytes.get(j) != Some(&b'=') || bytes.get(j + 1) != Some(&b'>') {
        return false;
    }
    j += 2;

    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }

    // Concise body (no '{') — no await is syntactically possible in a non-async arrow
    if bytes.get(j) != Some(&b'{') {
        return true;
    }

    is_single_return_block_no_await(&after[j..])
}

/// True when the block `{ … }` contains exactly one `return` statement and no
/// `await` keyword. Nested braces are tracked so inner objects/functions do not
/// terminate the scan early.
fn is_single_return_block_no_await(block: &str) -> bool {
    let bytes = block.as_bytes();
    if bytes.first() != Some(&b'{') {
        return false;
    }

    let mut depth = 0i32;
    let mut content_start = None;
    let mut content_end = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                if depth == 1 {
                    content_start = Some(i + 1);
                }
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    content_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let (start, end) = match (content_start, content_end) {
        (Some(s), Some(e)) => (s, e),
        _ => return false,
    };
    let content = block[start..end].trim();

    if content.contains("await") {
        return false;
    }

    // Must start with `return` followed by whitespace or `;`
    if !content.starts_with("return") {
        return false;
    }
    let after_return = &content["return".len()..];
    if !after_return.starts_with(|c: char| c.is_whitespace() || c == ';') {
        return false;
    }

    // One semicolon at depth-0 (the trailing `;`) means single statement
    let (mut angle, mut paren, mut bracket, mut brace) = (0i32, 0i32, 0i32, 0i32);
    let mut semicolons = 0usize;
    for b in after_return.bytes() {
        match b {
            b'<' => angle += 1,
            b'>' if angle > 0 => angle -= 1,
            b'(' => paren += 1,
            b')' if paren > 0 => paren -= 1,
            b'[' => bracket += 1,
            b']' if bracket > 0 => bracket -= 1,
            b'{' => brace += 1,
            b'}' if brace > 0 => brace -= 1,
            b';' if angle == 0 && paren == 0 && bracket == 0 && brace == 0 => semicolons += 1,
            _ => {}
        }
    }
    semicolons <= 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};

    fn fake_diag(path: &Path, line: usize, column: usize, rule: &'static str) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column,
            rule_id: Cow::Borrowed(rule),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-promise-fn-async-post-filter-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    fn line_col_of(src: &str, needle: &str) -> (usize, usize) {
        for (i, l) in src.lines().enumerate() {
            if let Some(c) = l.find(needle) {
                return (i + 1, c + 1);
            }
        }
        panic!("needle not in source: {needle}");
    }

    // Regression for #273: a function returning Effect.Effect<…> is not a
    // Promise — the diagnostic must be dropped.
    #[test]
    fn drops_effect_return_type() {
        let src = "function getUser(id: string): Effect.Effect<User, Err> {\n  return program;\n}\n";
        let path = write_temp("effect_fn.ts", src);
        let (line, col) = line_col_of(src, "function");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected drop, got: {diags:?}");
    }

    #[test]
    fn drops_effect_arrow_return_type() {
        let src = "const load = (): Effect.Effect<A, never, never> => program;\n";
        let path = write_temp("effect_arrow.ts", src);
        let (line, col) = line_col_of(src, "const");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected drop, got: {diags:?}");
    }

    #[test]
    fn keeps_promise_return_type() {
        let src = "function f(): Promise<void> {\n  return p;\n}\n";
        let path = write_temp("promise_fn.ts", src);
        let (line, col) = line_col_of(src, "function");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "Promise return must be kept");
    }

    #[test]
    fn keeps_promise_of_function_type() {
        // The inline `=>` inside the generic must not truncate the capture.
        let src = "function f(): Promise<() => void> {\n  return p;\n}\n";
        let path = write_temp("promise_fn_type.ts", src);
        let (line, col) = line_col_of(src, "function");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "Promise<fn> return must be kept");
    }

    #[test]
    fn keeps_when_no_annotation() {
        let src = "function f() {\n  return p;\n}\n";
        let path = write_temp("no_annotation.ts", src);
        let (line, col) = line_col_of(src, "function");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "unannotated fn must be kept (type-aware knows best)");
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = "function f(): Effect.Effect<A> {}\n";
        let path = write_temp("other_rule.ts", src);
        let (line, col) = line_col_of(src, "function");
        let mut diags = vec![
            fake_diag(&path, line, col, "promise-function-async"),
            fake_diag(&path, line, col, "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent = std::env::temp_dir().join("comply-pfa-missing.ts");
        let mut diags = vec![fake_diag(&nonexistent, 1, 1, "promise-function-async")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    // Regression for #342: concise callback arrow is a pass-through — no FP.
    // Oxlint reports the diagnostic at the '(' of the arrow's parameter list
    // (e.g. column 18 for `  return apiCall((api) => api.get());`).
    #[test]
    fn drops_concise_callback_arrow_passthrough() {
        let src = "apiCall((api) => api.get())\n";
        let path = write_temp("callback_passthrough.ts", src);
        let (line, col) = line_col_of(src, "(api)");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected drop, got: {diags:?}");
    }

    #[test]
    fn drops_concise_callback_arrow_multiline() {
        let src = "apiCall(\n  (api) =>\n    api.v1.products.get({ query })\n)\n";
        let path = write_temp("callback_multiline.ts", src);
        let (line, col) = line_col_of(src, "(api)");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected drop, got: {diags:?}");
    }

    #[test]
    fn drops_single_return_block_callback_arrow() {
        let src = "apiCall((api) => { return api.get(); })\n";
        let path = write_temp("block_callback.ts", src);
        let (line, col) = line_col_of(src, "(api)");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected drop, got: {diags:?}");
    }

    // The outer function wrapping an apiCall must still be flagged.
    #[test]
    fn keeps_outer_fn_returning_promise_indirectly() {
        let src =
            "function productsQueryOptions() {\n  return apiCall((api) => api.get());\n}\n";
        let path = write_temp("outer_fn.ts", src);
        let (line, col) = line_col_of(src, "function");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "outer block function must be kept");
    }

    // Arrow with multiple statements is not a simple pass-through — keep.
    #[test]
    fn keeps_multi_statement_block_arrow() {
        let src = "apiCall((api) => { const r = api.get(); return r; })\n";
        let path = write_temp("multi_stmt_block.ts", src);
        let (line, col) = line_col_of(src, "(api)");
        let mut diags = vec![fake_diag(&path, line, col, "promise-function-async")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "multi-statement block arrow must be kept");
    }
}
