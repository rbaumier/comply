use crate::diagnostic::{Diagnostic, Severity};

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
    let dot_count = inner.chars().filter(|c| *c == '.').count();
    let max_depth = ctx.config.threshold("i18n-max-key-depth", "max_depth", ctx.lang);
    if dot_count < max_depth { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &first,
        super::META.id,
        "t() key nests more than 2 levels deep. Flatten to `domain.key`.".into(),
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
    fn flags_deep_key() {
        assert_eq!(run("t('a.b.c.d')").len(), 1);
    }

    #[test]
    fn allows_two_levels() {
        assert!(run("t('auth.login.title')").is_empty());
    }

    #[test]
    fn allows_one_level() {
        assert!(run("t('auth.title')").is_empty());
    }
}
