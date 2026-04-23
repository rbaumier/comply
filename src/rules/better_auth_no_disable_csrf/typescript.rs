//! better-auth-no-disable-csrf — flag `disableCSRFCheck: true` in Better Auth config.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "pair" {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).unwrap_or("").trim_matches(|c: char| c == '\'' || c == '"');
    if key_text != "disableCSRFCheck" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.utf8_text(source).unwrap_or("").trim() != "true" {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`disableCSRFCheck: true` disables CSRF protection — remove this option.".into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_disable_csrf() {
        assert_eq!(run("betterAuth({ disableCSRFCheck: true })").len(), 1);
    }

    #[test]
    fn allows_csrf_enabled() {
        assert!(run("betterAuth({ database: db })").is_empty());
    }

    #[test]
    fn allows_csrf_false() {
        assert!(run("betterAuth({ disableCSRFCheck: false })").is_empty());
    }
}
