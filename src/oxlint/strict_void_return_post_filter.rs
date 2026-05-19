//! Post-filter for `strict-void-return` false positives on test-double
//! mocks and React-Testing-Library `renderHook` callbacks.
//!
//! Two FP shapes are dropped:
//!
//! 1. **`vi.fn()` mocks** — `vi.fn()` returns `Mock<Args, Return>` whose call
//!    signature is `(...args) => unknown`. Tests typically inspect
//!    `.mock.calls` and never the return value. Two forms are accepted: the
//!    inline `<Dialog onClose={vi.fn()} />` shape, and the aliased
//!    `const mock = vi.fn(); <Dialog onClose={mock} />` shape (resolved by
//!    locating a `const|let|var <name> = vi.fn(` declaration in the same
//!    file).
//!
//! 2. **`renderHook(() => useFoo())` callbacks** — the callback contract
//!    *requires* returning the hook value. The diagnostic is dropped when
//!    the enclosing call's callee is `renderHook` (or `<member>.renderHook`).

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "strict-void-return" {
            return true;
        }
        let entry = file_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_vi_fn_fp(src, d.line, d.column) && !is_render_hook_fp(src, d.line)
    });
}

/// True when the diagnostic location is a `vi.fn(...)` mock — either inline
/// at the diagnostic column, or an identifier whose declaration in the same
/// file is `const|let|var <name> = vi.fn(...)`.
fn is_vi_fn_fp(src: &str, line_1based: usize, column_1based: usize) -> bool {
    let Some(line) = src.lines().nth(line_1based.saturating_sub(1)) else {
        return false;
    };
    if has_vi_fn_call_at_or_after(line, column_1based) {
        return true;
    }
    let Some(ident) = identifier_at(line, column_1based) else {
        return false;
    };
    is_vi_fn_alias(src, &ident)
}

/// True when the source line carries `vi.fn(` at or after `column_1based`.
/// The diagnostic column may land on the JSX attribute name or the expression
/// itself — checking the rest of the line catches both.
/// `col0` is clamped to `line.len()`, so `line[line.len()..]` = `""` and
/// `contains` returns false safely when the column is past EOL.
fn has_vi_fn_call_at_or_after(line: &str, column_1based: usize) -> bool {
    let col0 = column_1based.saturating_sub(1).min(line.len());
    line[col0..].contains("vi.fn(")
}

/// Extract a JS/TS identifier starting at `column_1based` on `line`. Returns
/// the identifier text, or `None` if the column is not on an identifier.
///
/// `column_1based` is treated as a char offset (as reported by oxlint). When
/// multi-byte UTF-8 characters precede the column, the byte offset may not
/// coincide with a char boundary. Both `start` and `end` are validated with
/// `is_char_boundary` before slicing; if either is not a boundary, `None` is
/// returned rather than panicking.
fn identifier_at(line: &str, column_1based: usize) -> Option<String> {
    let bytes = line.as_bytes();
    if column_1based == 0 || column_1based > bytes.len() {
        return None;
    }
    let start = column_1based - 1;
    if !line.is_char_boundary(start) {
        return None;
    }
    if !is_ident_start(bytes[start]) {
        return None;
    }
    let mut end = start;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if !line.is_char_boundary(end) {
        return None;
    }
    Some(line[start..end].to_string())
}

/// True when `src` contains a declaration `const|let|var <ident> = vi.fn(`,
/// possibly with intervening whitespace or a TS type annotation.
fn is_vi_fn_alias(src: &str, ident: &str) -> bool {
    for line in src.lines() {
        let trimmed = line.trim_start();
        for kw in ["const ", "let ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(kw) {
                let rest = rest.trim_start();
                if let Some(after_ident) = rest.strip_prefix(ident) {
                    // Reject prefix matches: must be followed by non-ident.
                    let next = after_ident.as_bytes().first().copied();
                    if next.is_none_or(|b| !is_ident_byte(b)) && after_ident.contains("vi.fn(") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// True when the diagnostic at `line_1based` is inside a `renderHook(...)` or
/// `<member>.renderHook(...)` call. We look at the diagnostic line and the
/// line immediately above (2-line window). This covers both the inline shape
/// `renderHook(() => useFoo())` and the multiline shape where the callback
/// opener appears on the line after `renderHook(`. A wider window risks
/// silencing diagnostics from unrelated calls in the same test file.
fn is_render_hook_fp(src: &str, line_1based: usize) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    if line_1based > lines.len() {
        return false;
    }
    let start = line_1based.saturating_sub(1).max(1);
    for i in (start..=line_1based).rev() {
        let line = lines[i - 1];
        if contains_render_hook_call(line) {
            return true;
        }
    }
    false
}

/// True when `line` contains `renderHook(` not preceded by an identifier
/// character (so we don't match `myRenderHook(`). A leading `.` is fine —
/// matches `foo.renderHook(`.
fn contains_render_hook_call(line: &str) -> bool {
    let needle = "renderHook(";
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(pos) = line[from..].find(needle) {
        let abs = from + pos;
        let ok = abs == 0 || {
            let prev = bytes[abs - 1];
            !is_ident_byte(prev)
        };
        if ok {
            return true;
        }
        from = abs + needle.len();
    }
    false
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
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
        let dir = std::env::temp_dir().join("comply-strict-void-return-post-filter-tests");
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

    #[test]
    fn drops_inline_vi_fn_jsx_prop() {
        let src = "import { vi } from 'vitest';\nfunction Test() { return <Dialog onClose={vi.fn()} />; }\n";
        let path = write_temp("inline_vi_fn.tsx", src);
        let (line, _) = line_col_of(src, "vi.fn()");
        let mut diags = vec![fake_diag(&path, line, 1, "strict-void-return")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn drops_aliased_vi_fn_via_const_declaration() {
        let src = "import { vi } from 'vitest';\nconst onClose = vi.fn();\nfunction Test() { return <Dialog onClose={onClose} />; }\n";
        let path = write_temp("aliased_vi_fn.tsx", src);
        let (line, col) = line_col_of(src, "onClose={onClose}");
        // Column points to the JSX attr name; the rule reports on the value.
        // Use the value position instead.
        let value_col = col + "onClose={".len();
        let mut diags = vec![fake_diag(&path, line, value_col, "strict-void-return")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected aliased vi.fn drop, got: {diags:?}");
    }

    #[test]
    fn drops_aliased_vi_fn_with_let_declaration() {
        let src = "import { vi } from 'vitest';\nlet handler = vi.fn();\nfunction T() { return <Dialog onClose={handler} />; }\n";
        let path = write_temp("aliased_vi_fn_let.tsx", src);
        let (line, col) = line_col_of(src, "{handler}");
        let value_col = col + 1; // past `{`
        let mut diags = vec![fake_diag(&path, line, value_col, "strict-void-return")];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn drops_render_hook_callback() {
        let src = "import { renderHook } from '@testing-library/react';\ntest('x', () => {\n  renderHook(() => useUser());\n});\n";
        let path = write_temp("render_hook.tsx", src);
        let (line, _) = line_col_of(src, "renderHook(() =>");
        let mut diags = vec![fake_diag(&path, line, 15, "strict-void-return")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected renderHook drop, got: {diags:?}");
    }

    #[test]
    fn drops_render_hook_callback_multiline() {
        let src = "import { renderHook } from '@testing-library/react';\ntest('x', () => {\n  renderHook(() => {\n    return useUser();\n  });\n});\n";
        let path = write_temp("render_hook_multi.tsx", src);
        // Diagnostic anchored on the inner return statement line.
        let (line, _) = line_col_of(src, "return useUser()");
        let mut diags = vec![fake_diag(&path, line, 5, "strict-void-return")];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn drops_member_render_hook() {
        let src = "import * as tl from '@testing-library/react';\ntest('x', () => {\n  tl.renderHook(() => useUser());\n});\n";
        let path = write_temp("member_render_hook.tsx", src);
        let (line, _) = line_col_of(src, "tl.renderHook(");
        let mut diags = vec![fake_diag(&path, line, 18, "strict-void-return")];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn keeps_genuine_void_misuse() {
        // Plain callback returning a value where `() => void` is expected.
        let src = "function setup(cb: () => void) { cb(); }\nsetup(() => 42);\n";
        let path = write_temp("genuine_misuse.ts", src);
        let (line, _) = line_col_of(src, "() => 42");
        let mut diags = vec![fake_diag(&path, line, 7, "strict-void-return")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected diagnostic kept, got: {diags:?}");
    }

    #[test]
    fn keeps_lookalike_my_render_hook() {
        // `myRenderHook(` must not match `renderHook(`.
        let src = "function myRenderHook(cb: () => void) { cb(); }\nmyRenderHook(() => 42);\n";
        let path = write_temp("my_render_hook.ts", src);
        let (line, _) = line_col_of(src, "myRenderHook(() => 42)");
        let mut diags = vec![fake_diag(&path, line, 14, "strict-void-return")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_diagnostic_when_alias_resolves_elsewhere() {
        // `mock` is declared but NOT from vi.fn — must not be silenced.
        let src = "const mock = () => 42;\nfunction Test() { return <Dialog onClose={mock} />; }\n";
        let path = write_temp("non_vi_alias.tsx", src);
        let (line, col) = line_col_of(src, "{mock}");
        let value_col = col + 1;
        let mut diags = vec![fake_diag(&path, line, value_col, "strict-void-return")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected diagnostic kept: {diags:?}");
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = "import { vi } from 'vitest';\nconst x = vi.fn();\n";
        let path = write_temp("other_rule.ts", src);
        let mut diags = vec![
            fake_diag(&path, 2, 11, "strict-void-return"),
            fake_diag(&path, 2, 11, "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent = std::env::temp_dir().join("comply-svr-missing.ts");
        let mut diags = vec![fake_diag(&nonexistent, 42, 1, "strict-void-return")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    /// Regression: a `renderHook` block must not bleed into an unrelated call
    /// that appears 3+ lines below it (beyond the 2-line window). The old
    /// 20-line window would have silenced the `otherCall` diagnostic; the new
    /// 2-line window must not.
    #[test]
    fn render_hook_does_not_bleed_into_adjacent_unrelated_call() {
        // Line 3: renderHook block (should be dropped).
        // Lines 4-5: gap (blank + comment).
        // Line 6: otherCall block (should be KEPT — more than 1 line below renderHook).
        let src = "\
import { renderHook } from '@testing-library/react';
test('a', () => {
  renderHook(() => useUser());

  // unrelated
  otherCall(() => getValue());
});
";
        let path = write_temp("render_hook_no_bleed.tsx", src);
        // Diagnostic on the renderHook callback — should be dropped.
        let (rh_line, _) = line_col_of(src, "renderHook(() =>");
        // Diagnostic on the otherCall callback — should be KEPT.
        // It is 3 lines below renderHook(, outside the 2-line window.
        let (oc_line, _) = line_col_of(src, "otherCall(() =>");
        assert!(
            oc_line > rh_line + 1,
            "test setup: otherCall must be >1 line below renderHook (rh={rh_line}, oc={oc_line})"
        );
        let mut diags = vec![
            fake_diag(&path, rh_line, 15, "strict-void-return"),
            fake_diag(&path, oc_line, 13, "strict-void-return"),
        ];
        apply(&mut diags);
        assert_eq!(
            diags.len(),
            1,
            "otherCall diag must survive, renderHook diag must be dropped; got: {diags:?}"
        );
        assert!(
            diags[0].line == oc_line,
            "surviving diag must be otherCall on line {oc_line}, got line {}",
            diags[0].line
        );
    }

    /// Regression: `identifier_at` must not panic on a line with multi-byte
    /// UTF-8 characters before the column. It should return `Some(ident)` if
    /// the column lands on a valid ASCII identifier boundary, or `None`
    /// gracefully — never panic.
    #[test]
    fn identifier_at_no_panic_with_multibyte_utf8() {
        // Line contains multi-byte chars before the identifier `myFn`.
        let line = "// commenté avec é à â myFn(";
        // Find the byte offset of `myFn`.
        let byte_start = line.find("myFn").expect("myFn must be in line");
        // column_1based is byte_start + 1.
        let col = byte_start + 1;
        // Must not panic; result is either Some("myFn") or None.
        let result = identifier_at(line, col);
        // Verify the result is correct when it succeeds.
        if let Some(ident) = result {
            assert_eq!(ident, "myFn", "expected 'myFn', got '{ident}'");
        }
        // If None — also acceptable (graceful degradation), no panic is the key assertion.
    }

    #[test]
    fn ident_prefix_does_not_match_alias() {
        // `const mockable = vi.fn()` must not satisfy lookup for `mock`.
        let src = "import { vi } from 'vitest';\nconst mockable = vi.fn();\nfunction T() { return <D onClose={mock} />; }\n";
        let path = write_temp("prefix_no_match.tsx", src);
        let (line, col) = line_col_of(src, "{mock}");
        let value_col = col + 1;
        let mut diags = vec![fake_diag(&path, line, value_col, "strict-void-return")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "prefix match must not silence: {diags:?}");
    }
}
