//! Post-filter for `no-redundant-type-constituents` false positives on the
//! `keyof T & string` narrowing pattern.
//!
//! `keyof T & string` is a standard TypeScript idiom to extract only the
//! string keys of a type (filtering out `number` and `symbol` keys that
//! `keyof T` can include). When every key of `T` happens to be a string
//! (e.g. `{ id: string; email: string }`), tsgolint resolves the intersection
//! to the literal union and concludes that the `& string` constituent is
//! redundant — it is not. The `& string` guard is intentional: it prevents
//! accidental use of numeric / symbol keys in string-typed positions.
//!
//! The diagnostic fires once per array element when the pattern appears in a
//! `satisfies readonly (keyof T & string)[]` clause, because tsgolint checks
//! each element against the resolved intersection type. Drop it whenever the
//! diagnostic line or a small window around it contains the `keyof … & string`
//! pattern.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

/// Window size (number of lines before and after the diagnostic line) to scan
/// for the `keyof … & string` pattern. Five lines is enough to cover a
/// multi-line array literal whose `satisfies` clause is on the last line.
const WINDOW: usize = 5;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "no-redundant-type-constituents" {
            return true;
        }
        let entry = file_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_keyof_string_narrowing_fp(src, d.line)
    });
}

/// True when a `keyof … & string` type expression appears within `WINDOW`
/// lines of the diagnostic. The `& string` constituent in this idiom is
/// intentional and must not be flagged.
fn is_keyof_string_narrowing_fp(src: &str, line_1based: usize) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    let lo = line_1based.saturating_sub(WINDOW + 1);
    let hi = (line_1based + WINDOW).min(lines.len());
    lines[lo..hi]
        .iter()
        .any(|line| has_keyof_string_intersection(line))
}

/// True when the line contains `keyof` (as a standalone keyword) followed by
/// `& string` (where `string` is not immediately followed by an identifier
/// character). Handles both `keyof T & string` and `(keyof T & string)`.
fn has_keyof_string_intersection(line: &str) -> bool {
    if !line.contains("keyof") {
        return false;
    }
    let bytes = line.as_bytes();
    // Search for `& string` where `string` is a whole type (not e.g. `stringFoo`).
    let needle = b"& string";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if bytes[i..i + needle.len()] == *needle {
            let after = i + needle.len();
            let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
            if after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
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

    fn fake_diag(path: &Path, line: usize, rule: &'static str) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed(rule),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("comply-no-redundant-type-constituents-post-filter-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    fn line_of(src: &str, needle: &str) -> usize {
        src.lines()
            .enumerate()
            .find(|(_, l)| l.contains(needle))
            .map(|(i, _)| i + 1)
            .expect("needle not in source")
    }

    // ── Regression tests (issue #553) ──────────────────────────────────────

    /// Single-line array with satisfies keyof T & string — all elements fire.
    #[test]
    fn drops_single_line_keyof_string_satisfies() {
        let src = r#"type User = { id: string; email: string; createdAt: Date };
const sortable = ['email', 'createdAt'] as const satisfies readonly (keyof User & string)[];
"#;
        let path = write_temp("single_line_keyof.ts", src);
        let line = line_of(src, "satisfies");
        let mut diags = vec![
            fake_diag(&path, line, "no-redundant-type-constituents"),
            fake_diag(&path, line, "no-redundant-type-constituents"),
        ];
        apply(&mut diags);
        assert!(diags.is_empty(), "all keyof & string FPs must be dropped: {diags:?}");
    }

    /// Multi-line array where the satisfies clause is on the last line; the
    /// diagnostics fire on the element lines (up to WINDOW lines above).
    #[test]
    fn drops_multiline_array_keyof_string_satisfies() {
        let src = concat!(
            "type User = { id: string; email: string; createdAt: Date };\n",
            "const sortable = [\n",
            "  'id',\n",
            "  'email',\n",
            "  'createdAt',\n",
            "] as const satisfies readonly (keyof User & string)[];\n",
        );
        let path = write_temp("multiline_keyof.ts", src);
        // tsgolint fires on each element line
        let l1 = line_of(src, "'id'");
        let l2 = line_of(src, "'email'");
        let l3 = line_of(src, "'createdAt'");
        let mut diags = vec![
            fake_diag(&path, l1, "no-redundant-type-constituents"),
            fake_diag(&path, l2, "no-redundant-type-constituents"),
            fake_diag(&path, l3, "no-redundant-type-constituents"),
        ];
        apply(&mut diags);
        assert!(diags.is_empty(), "all 3 FPs in multiline array must be dropped: {diags:?}");
    }

    // ── Negative tests — real violations must still fire ───────────────────

    /// `string | 'foo'` is a genuine redundant-type-constituent (literal
    /// subsumed by its supertype). Must not be suppressed.
    #[test]
    fn keeps_string_union_literal() {
        let src = "type T = string | 'foo';\n";
        let path = write_temp("real_union_literal.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "no-redundant-type-constituents")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "real violation must be kept");
    }

    /// `string & string` is genuinely redundant — not a keyof pattern.
    #[test]
    fn keeps_string_intersection_string() {
        let src = "type T = string & string;\n";
        let path = write_temp("string_and_string.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "no-redundant-type-constituents")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "string & string must be kept");
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = r#"type User = { id: string };
const x = ['id'] as const satisfies readonly (keyof User & string)[];
"#;
        let path = write_temp("other_rule.ts", src);
        let line = line_of(src, "satisfies");
        let mut diags = vec![
            fake_diag(&path, line, "no-redundant-type-constituents"),
            fake_diag(&path, line, "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "only no-explicit-any must remain");
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent =
            std::env::temp_dir().join("does-not-exist-comply-no-redundant-type-test.ts");
        let mut diags = vec![fake_diag(&nonexistent, 1, "no-redundant-type-constituents")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }
}
