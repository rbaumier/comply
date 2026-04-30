use crate::diagnostic::{Diagnostic, Severity};

fn is_malformed(inner: &str) -> bool {
    if inner.is_empty() {
        return false;
    }
    if inner.contains("..") || inner.ends_with('.') || inner.starts_with('.') {
        return true;
    }
    // Empty segments inside (defense in depth — `..` already covers this).
    if inner.split('.').any(str::is_empty) {
        return true;
    }
    // Reject any character that isn't alphanumeric, dot, hyphen, or underscore.
    // This catches slashes, spaces, punctuation, and other separators that
    // can never resolve in a flat `auth.title`-style locale tree.
    if inner
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'))
    {
        return true;
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    let func_text = func.utf8_text(source).unwrap_or("");
    if func_text != "t" && func_text != "i18n.t" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if first.kind() != "string" { return; }
    let Ok(raw) = first.utf8_text(source) else { return };
    let inner = raw
        .strip_prefix('"').and_then(|s| s.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(raw);
    if !is_malformed(inner) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &first,
        super::META.id,
        "t() key is malformed (consecutive/leading/trailing dots, empty segment, or non-alphanumeric character) — it cannot resolve to a locale entry.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_double_dot() {
        assert_eq!(run("t('auth..title')").len(), 1);
    }

    #[test]
    fn flags_trailing_dot() {
        assert_eq!(run("t('auth.title.')").len(), 1);
    }

    #[test]
    fn allows_normal_key() {
        assert!(run("t('auth.title')").is_empty());
    }

    #[test]
    fn flags_leading_dot() {
        assert_eq!(run("t('.auth.title')").len(), 1);
    }

    #[test]
    fn flags_slash_in_key() {
        assert_eq!(run("t('auth/title')").len(), 1);
    }

    #[test]
    fn flags_special_char_in_key() {
        assert_eq!(run("t('auth.title!')").len(), 1);
    }
}
