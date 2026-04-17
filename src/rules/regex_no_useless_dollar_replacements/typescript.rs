//! regex-no-useless-dollar-replacements TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only. Flags a regex literal passed as
//! the first argument to `<expr>.replace(...)` / `<expr>.replaceAll(...)`
//! when the sibling replacement string contains `$N` references that
//! exceed the number of capturing groups in the regex.
//!
//! Gating by AST eliminates the false-positive class from the previous
//! TextCheck (which could match regex-like substrings inside Tailwind
//! classes, URLs and scoped import paths such as `"@scope/pkg"`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Methods whose first argument is a regex paired with a replacement string.
const REPLACE_METHODS: &[&str] = &["replace", "replaceAll"];

/// Count capturing groups in a regex pattern. Non-capturing groups `(?:...)`,
/// lookarounds `(?=...)` / `(?!...)` / `(?<=...)` / `(?<!...)` and named
/// groups `(?<name>...)` (still capturing) are handled correctly. Escaped
/// parens and parens inside character classes are ignored.
fn count_capturing_groups(pattern: &str) -> usize {
    let bytes = pattern.as_bytes();
    let mut groups = 0usize;
    let mut i = 0;
    let mut in_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                i += 2;
                continue;
            }
            b'[' if !in_class => {
                in_class = true;
                i += 1;
                continue;
            }
            b']' if in_class => {
                in_class = false;
                i += 1;
                continue;
            }
            b'(' if !in_class => {
                // `(?...)` — non-capturing variants unless it's `(?<name>...)`.
                if i + 1 < bytes.len() && bytes[i + 1] == b'?' {
                    // `(?<name>...)` is capturing; `(?<=...)` / `(?<!...)` are not.
                    if i + 2 < bytes.len()
                        && bytes[i + 2] == b'<'
                        && i + 3 < bytes.len()
                        && bytes[i + 3] != b'='
                        && bytes[i + 3] != b'!'
                    {
                        groups += 1;
                    }
                    // All other `(?...)` forms are non-capturing.
                } else {
                    groups += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    groups
}

/// Scan a replacement string for the highest `$N` numeric reference. Returns
/// `0` when no numeric reference is present. `$$` (literal dollar) is
/// respected. `$&`, `$'`, `` $` ``, `$<name>` are not numeric references.
fn max_dollar_numeric_ref(replacement: &str) -> usize {
    let bytes = replacement.as_bytes();
    let mut max_ref = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'$' {
                // Escaped `$$` — skip both.
                i += 2;
                continue;
            }
            if next.is_ascii_digit() {
                // Parse one or two digit number (`$NN` where first digit is
                // non-zero and the two-digit form <= group count is read as
                // one number by the spec; we take the greedy upper-bound
                // which is the safe direction for this warning).
                let mut n = (next - b'0') as usize;
                let mut consumed = 2;
                if i + 2 < bytes.len() && bytes[i + 2].is_ascii_digit() {
                    let two = n * 10 + (bytes[i + 2] - b'0') as usize;
                    // Only accept two-digit form if leading digit isn't 0.
                    if n != 0 {
                        n = two;
                        consumed = 3;
                    }
                }
                if n > max_ref {
                    max_ref = n;
                }
                i += consumed;
                continue;
            }
        }
        i += 1;
    }
    max_ref
}

/// Concatenate `string_fragment` children of a `template_string` into a
/// single string so the `$N` scan can run on template literals too.
/// `template_substitution` segments are replaced with a single placeholder
/// character that cannot form a `$N` reference.
fn template_static_text(node: tree_sitter::Node<'_>, source: &[u8]) -> String {
    let mut cursor = node.walk();
    let mut out = String::new();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "string_fragment" => {
                if let Ok(t) = child.utf8_text(source) {
                    out.push_str(t);
                }
            }
            "template_substitution" => out.push(' '),
            _ => {}
        }
    }
    out
}

/// Returns the static text of a string-like argument node, or `None` if
/// the node isn't a static string (e.g. a variable or expression).
fn static_string_text(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "string" => {
            // A `string` node is `"..."` / `'...'` with a `string_fragment`
            // child holding the inner text.
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "string_fragment" {
                    return child.utf8_text(source).ok().map(str::to_owned);
                }
            }
            // Empty string literal (no `string_fragment` child).
            Some(String::new())
        }
        "template_string" => Some(template_static_text(node, source)),
        _ => None,
    }
}

/// Walk up from a `regex` node to the enclosing `call_expression`, skipping
/// optional `parenthesized_expression` wrappers.
fn containing_call<'t>(node: tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    let mut current = node.parent()?;
    while current.kind() == "parenthesized_expression" {
        current = current.parent()?;
    }
    if current.kind() != "arguments" {
        return None;
    }
    let call = current.parent()?;
    if call.kind() != "call_expression" {
        return None;
    }
    Some(call)
}

/// Property name of `<object>.<property>(...)` call, or `None` if the
/// callee isn't a member expression.
fn called_method_name<'a>(call: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "member_expression" {
        return None;
    }
    let prop = func.child_by_field_name("property")?;
    prop.utf8_text(source).ok()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    let Some(call) = containing_call(node) else { return };
    let Some(method) = called_method_name(call, source) else { return };
    if !REPLACE_METHODS.contains(&method) {
        return;
    }
    // Locate the regex as the first argument and the replacement as the
    // second named child of the `arguments` list.
    let Some(args) = call.child_by_field_name("arguments") else { return };
    if args.named_child_count() < 2 {
        return;
    }
    let Some(first_arg) = args.named_child(0) else { return };
    if first_arg.id() != node.id() {
        // The regex isn't the first argument — skip (e.g. nested calls).
        return;
    }
    let Some(second_arg) = args.named_child(1) else { return };
    let Some(replacement) = static_string_text(second_arg, source) else { return };
    let max_ref = max_dollar_numeric_ref(&replacement);
    if max_ref == 0 {
        return;
    }
    let group_count = count_capturing_groups(pattern);
    if max_ref <= group_count {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-dollar-replacements",
        "Replacement string references a capturing group that does not exist in the regex.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_nonexistent_group_ref() {
        assert_eq!(run_on(r#"str.replace(/(a)/, "$2");"#).len(), 1);
    }

    #[test]
    fn allows_valid_group_ref() {
        assert!(run_on(r#"str.replace(/(a)/, "$1");"#).is_empty());
    }

    #[test]
    fn flags_replaceall_nonexistent() {
        assert_eq!(run_on(r#"str.replaceAll(/(a)/g, "$3");"#).len(), 1);
    }

    #[test]
    fn allows_no_groups_no_refs() {
        assert!(run_on(r#"str.replace(/a/, "b");"#).is_empty());
    }

    #[test]
    fn allows_escaped_dollar() {
        // `$$` is a literal dollar sign, not a numeric reference.
        assert!(run_on(r#"str.replace(/(a)/, "$$1");"#).is_empty());
    }

    #[test]
    fn ignores_non_capturing_groups() {
        // `(?:...)` doesn't contribute to the group count.
        assert_eq!(run_on(r#"str.replace(/(?:a)/, "$1");"#).len(), 1);
    }

    #[test]
    fn respects_named_capturing_groups() {
        // `(?<name>...)` is still a capturing group (referenceable as `$1`).
        assert!(run_on(r#"str.replace(/(?<name>a)/, "$1");"#).is_empty());
    }

    #[test]
    fn ignores_lookahead_groups() {
        // `(?=...)` is not capturing.
        assert_eq!(run_on(r#"str.replace(/(?=a)/, "$1");"#).len(), 1);
    }

    #[test]
    fn flags_template_literal_replacement() {
        assert_eq!(run_on(r#"str.replace(/(a)/, `$2`);"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
