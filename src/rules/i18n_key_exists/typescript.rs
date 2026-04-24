use crate::diagnostic::{Diagnostic, Severity};

fn is_malformed(inner: &str) -> bool {
    inner.contains("..") || inner.ends_with('.') || inner.starts_with('.')
}

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
    if !is_malformed(inner) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &first,
        super::META.id,
        "t() key is malformed (double dot, leading dot, or trailing dot) — it will not match any locale entry.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(s, &Check) }

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
}
