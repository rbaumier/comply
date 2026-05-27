//! Post-filter for `promise-function-async` false positives on functions whose
//! explicit return type is not a Promise.
//!
//! `promise-function-async` mandates the `async` keyword on Promise-returning
//! functions. In an effect-ts codebase, functions return `Effect.Effect<…>`,
//! which is *not* a Promise — making them `async` would wrap the Effect in a
//! Promise and break the program. When a function carries an explicit return
//! type annotation that does not mention `Promise`/`PromiseLike`, the
//! diagnostic is dropped. Functions with no annotation (the type-aware checker
//! knows best) and genuine `Promise<…>` returns are left untouched.

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
}
