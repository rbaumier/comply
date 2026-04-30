use crate::diagnostic::{Diagnostic, Severity};

/// Validate a key against `^[a-z][a-zA-Z0-9]*(\.[a-z][a-zA-Z0-9]*)+$`:
/// - at least 2 segments separated by `.`,
/// - each segment starts with lowercase + only ASCII alphanumerics,
/// - no consecutive dots, no slashes, no other separators.
fn is_valid_namespaced(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    let segments: Vec<&str> = key.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    for seg in &segments {
        if seg.is_empty() {
            return false; // catches `..`, leading `.`, trailing `.`
        }
        let mut chars = seg.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() {
            return false;
        }
        for c in chars {
            if !c.is_ascii_alphanumeric() {
                return false;
            }
        }
    }
    true
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
    if inner.is_empty() { return; }
    // Skip sentence-style keys — i18n-no-english-key owns that case.
    if inner.contains(' ') { return; }
    if is_valid_namespaced(inner) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &first,
        super::META.id,
        "t() key must match `domain.subkey` (lowercase-leading segments, dot-separated).".into(),
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
    fn flags_missing_domain() {
        assert_eq!(run("t('title')").len(), 1);
    }

    #[test]
    fn allows_domain_key() {
        assert!(run("t('auth.title')").is_empty());
    }

    #[test]
    fn flags_uppercase_leading_segment() {
        // `Auth.Title` has uppercase-leading segments — invalid.
        assert_eq!(run("t('Auth.Title')").len(), 1);
    }

    #[test]
    fn flags_consecutive_dots() {
        assert_eq!(run("t('auth..title')").len(), 1);
    }

    #[test]
    fn flags_slash_separator() {
        assert_eq!(run("t('auth/title')").len(), 1);
    }
}
