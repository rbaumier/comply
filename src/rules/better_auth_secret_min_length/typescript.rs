//! better-auth-secret-min-length — flag short string literals for `secret:` config.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');
    if key_text != "secret" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "string" {
        return;
    }

    let raw = value.utf8_text(source).unwrap_or("");
    let inner = raw.trim_matches(|c: char| c == '\'' || c == '"' || c == '`');
    if inner.len() >= 32 {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`secret` is shorter than 32 characters — use a strong 32+ char secret.".into(),
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
    fn flags_short_secret() {
        assert_eq!(run("betterAuth({ secret: \"short\" })").len(), 1);
    }

    #[test]
    fn allows_long_secret() {
        assert!(
            run("betterAuth({ secret: \"a-very-long-secret-value-with-32-chars\" })").is_empty()
        );
    }

    #[test]
    fn ignores_env_secret() {
        assert!(run("betterAuth({ secret: process.env.BETTER_AUTH_SECRET })").is_empty());
    }
}
