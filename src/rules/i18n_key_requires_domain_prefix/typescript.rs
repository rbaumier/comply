use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
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
    if inner.contains('.') { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &first,
        super::META.id,
        "t() key must be namespaced under a domain (e.g. `auth.title`).".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(s, &Check) }

    #[test]
    fn flags_missing_domain() {
        assert_eq!(run("t('title')").len(), 1);
    }

    #[test]
    fn allows_domain_key() {
        assert!(run("t('auth.title')").is_empty());
    }
}
