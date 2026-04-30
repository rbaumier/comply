use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    let func_text = func.utf8_text(source).unwrap_or("");
    if func_text != "t" && func_text != "i18n.t" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if first.kind() != "string" { return; }
    let Ok(raw) = first.utf8_text(source) else { return };
    // Strip surrounding quotes.
    let inner = raw
        .strip_prefix('"').and_then(|s| s.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(raw);
    if inner.is_empty() { return; }
    let starts_upper = inner.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    let has_space = inner.contains(' ');
    if !starts_upper && !has_space { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &first,
        super::META.id,
        "t() key looks like an English sentence. Use an identifier-style key (e.g. `domain.key`) and store the copy in the locale file.".into(),
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
    fn flags_sentence_key() {
        assert_eq!(run("t('Hello world')").len(), 1);
    }

    #[test]
    fn flags_uppercase_start() {
        assert_eq!(run("t('Welcome')").len(), 1);
    }

    #[test]
    fn allows_identifier_key() {
        assert!(run("t('auth.login.title')").is_empty());
    }
}
