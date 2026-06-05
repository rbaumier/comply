//! Post-filter for `ban-types` false positives on the `T & {}` intersection
//! idiom used to widen a literal-string union while preserving autocomplete.
//!
//! `string & {}` is a well-known TypeScript pattern: it lets `"sm" | "md" |
//! string & {}` accept any string value while still surfacing the known
//! literals in editor completions. The `{}` here is an intersection operand,
//! not a standalone empty-object type annotation, so `ban-types` firing on it
//! is a false positive.
//!
//! Detection: inspect the full source line at `d.line`. Suppress when the line
//! contains `& {}` or `{} &` (with any amount of ASCII whitespace between
//! tokens). Standalone `: {}` annotations contain neither pattern and are
//! kept. (Closes #748)

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "ban-types" {
            return true;
        }
        if d.line == 0 {
            return true;
        }
        let entry = file_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        let line = src.lines().nth(d.line - 1).unwrap_or("");
        !is_intersection_member(line)
    });
}

/// True when the line contains `& {}` or `{} &` (with optional ASCII
/// whitespace between tokens), indicating `{}` is an intersection operand
/// rather than a standalone empty-object type.
fn is_intersection_member(line: &str) -> bool {
    let bytes = line.as_bytes();
    has_ampersand_then_empty_braces(bytes) || has_empty_braces_then_ampersand(bytes)
}

/// Scan for `&` followed by optional whitespace followed by `{}`.
fn has_ampersand_then_empty_braces(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j + 1 < bytes.len() && bytes[j] == b'{' && bytes[j + 1] == b'}' {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Scan for `{}` followed by optional whitespace followed by `&`.
fn has_empty_braces_then_ampersand(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'}' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'&' {
                return true;
            }
        }
        i += 1;
    }
    false
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
        let dir = std::env::temp_dir().join("comply-ban-types-post-filter-tests");
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

    // ── Regression tests (issue #748) ─────────────────────────────────────

    /// `type Spec = Breakpoint | (string & {})` in alias RHS — must be suppressed.
    #[test]
    fn drops_ban_types_on_alias_rhs_intersection() {
        let src = concat!(
            "type Breakpoint = \"sm\" | \"md\" | \"lg\";\n",
            "type Spec = Breakpoint | (string & {});\n",
        );
        let path = write_temp("alias_rhs_intersection.ts", src);
        let line = line_of(src, "string & {}");
        let mut diags = vec![fake_diag(&path, line, "ban-types")];
        apply(&mut diags);
        assert!(diags.is_empty(), "alias-RHS intersection FP must be suppressed: {diags:?}");
    }

    /// `function f(x: Breakpoint | (string & {}))` in parameter — must be suppressed.
    #[test]
    fn drops_ban_types_on_param_intersection() {
        let src = concat!(
            "type Breakpoint = \"sm\" | \"md\" | \"lg\";\n",
            "function f(x: Breakpoint | (string & {})): void {}\n",
        );
        let path = write_temp("param_intersection.ts", src);
        let line = line_of(src, "string & {}");
        let mut diags = vec![fake_diag(&path, line, "ban-types")];
        apply(&mut diags);
        assert!(diags.is_empty(), "param-annotation intersection FP must be suppressed: {diags:?}");
    }

    /// Left-operand form `{} & string` — must be suppressed.
    #[test]
    fn drops_ban_types_on_left_operand_form() {
        let src = "type T = ({} & string) | \"sm\";\n";
        let path = write_temp("left_operand_form.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "ban-types")];
        apply(&mut diags);
        assert!(diags.is_empty(), "{{}} & string left-operand form must be suppressed: {diags:?}");
    }

    // ── Negative tests — genuine violations must still fire ───────────────

    /// Standalone `: {}` annotation is a genuine `ban-types` violation — must be kept.
    #[test]
    fn keeps_standalone_empty_object_annotation() {
        let src = "const x: {} = foo;\n";
        let path = write_temp("standalone_empty_object.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "ban-types")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "standalone {{}} annotation must remain flagged");
    }

    /// A different rule on the same intersection line must not be suppressed.
    #[test]
    fn does_not_touch_other_rules() {
        let src = "type Spec = \"sm\" | (string & {});\n";
        let path = write_temp("other_rule_on_intersection.ts", src);
        let mut diags = vec![
            fake_diag(&path, 1, "ban-types"),
            fake_diag(&path, 1, "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "only no-explicit-any must remain");
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    /// Diagnostic on an unreadable file must be kept (safe fallback).
    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent =
            std::env::temp_dir().join("does-not-exist-comply-ban-types-post-filter.ts");
        let mut diags = vec![fake_diag(&nonexistent, 1, "ban-types")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "unreadable file diagnostic must be kept");
    }
}
