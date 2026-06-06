//! regex-no-unused-groups TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! string literals containing `(?<...>` lookalikes (URLs, templated
//! strings, scoped package names, Tailwind arbitrary values) cannot
//! false-positive as regex named capturing groups.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Extract named capturing group names and their byte offset within
/// `pattern`. Skips lookbehind constructs `(?<=...)` and `(?<!...)`.
fn extract_named_groups(pattern: &str) -> Vec<(String, usize)> {
    let mut groups = Vec::new();
    let bytes = pattern.as_bytes();
    let needle = b"(?<";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] != needle {
            i += 1;
            continue;
        }
        // Respect backslash escaping of `(`.
        let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
        if backslashes % 2 != 0 {
            i += 1;
            continue;
        }
        let name_start = i + needle.len();
        // Skip lookbehind `(?<=` / `(?<!`.
        if name_start < bytes.len() && (bytes[name_start] == b'=' || bytes[name_start] == b'!') {
            i = name_start;
            continue;
        }
        // Extract name up to `>`.
        if let Some(rel_end) = pattern[name_start..].find('>') {
            let name = &pattern[name_start..name_start + rel_end];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                groups.push((name.to_string(), i));
            }
            i = name_start + rel_end + 1;
        } else {
            break;
        }
    }
    groups
}

/// Checks whether a named group `name` is referenced somewhere in the
/// file — through `.groups.name`, `groups["name"]`, `groups['name']`,
/// or `$<name>` inside a replacement string.
fn is_group_referenced(name: &str, source: &str) -> bool {
    let dot_access = format!(".groups.{name}");
    let bracket_access_dq = format!("groups[\"{name}\"]");
    let bracket_access_sq = format!("groups['{name}']");
    let replacement_ref = format!("$<{name}>");
    crate::oxc_helpers::source_contains(source, &dot_access)
        || crate::oxc_helpers::source_contains(source, &bracket_access_dq)
        || crate::oxc_helpers::source_contains(source, &bracket_access_sq)
        || crate::oxc_helpers::source_contains(source, &replacement_ref)
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    let groups = extract_named_groups(pattern);
    if groups.is_empty() {
        return;
    }
    for (name, _offset) in groups {
        if is_group_referenced(&name, ctx.source) {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "regex-no-unused-groups",
            format!(
                "Named capturing group `{name}` is never referenced \u{2014} use `.groups.{name}` or convert to `(?:...)`."
            ),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_unused_named_group() {
        let src = r#"const re = /(?<year>\d{4})-(?<month>\d{2})/;"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_used_named_group_dot() {
        let src = "const re = /(?<year>\\d{4})/;\nconst y = match.groups.year;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_used_named_group_replacement() {
        let src = r#"const re = /(?<day>\d{2})/; str.replace(re, "$<day>");"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_used_named_group_bracket_double_quote() {
        let src = r#"const re = /(?<k>\w+)/; const v = match.groups["k"];"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_used_named_group_bracket_single_quote() {
        let src = r#"const re = /(?<k>\w+)/; const v = match.groups['k'];"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_lookbehind() {
        let src = r#"const re = /(?<=foo)bar/;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_negative_lookbehind() {
        let src = r#"const re = /(?<!foo)bar/;"#;
        assert!(run_on(src).is_empty());
    }

    // --- Regression tests for TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        // The string `(?<foo>` in a Tailwind-style class-like value must
        // not be seen as a regex named group since it's not a regex literal.
        let src = r#"const x = "grid-[(?<foo>_auto)]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_named_group_lookalike_in_url_string() {
        // URL-ish text containing `(?<name>` inside a plain string
        // literal must not be treated as a regex.
        let src = r#"const u = "https://a/b?x=(?<name>value)";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_named_group_in_scoped_import_path() {
        // Scoped import path containing `(?<x>` lookalike must not
        // be treated as a regex.
        let src = r#"import X from "@scope/(?<x>pkg)";"#;
        assert!(run_on(src).is_empty());
    }
}
