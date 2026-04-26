//! better-auth-trusted-providers — flag `accountLinking: { enabled: true, ... }`
//! that omits `trustedProviders`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).unwrap_or("").trim_matches(|c: char| c == '\'' || c == '"');
    if key_text != "accountLinking" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "object" {
        return;
    }

    let value_text = value.utf8_text(source).unwrap_or("");
    // Only flag when linking is explicitly enabled.
    if !value_text.contains("enabled: true") && !value_text.contains("enabled:true") {
        return;
    }
    if value_text.contains("trustedProviders") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`accountLinking` is enabled without `trustedProviders` — any OAuth provider can link accounts. Add `trustedProviders` to restrict this.".into(),
        Severity::Warning,
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
    fn flags_linking_without_trusted() {
        assert_eq!(
            run("betterAuth({ accountLinking: { enabled: true } })").len(),
            1
        );
    }

    #[test]
    fn allows_linking_with_trusted_providers() {
        assert!(run(
            "betterAuth({ accountLinking: { enabled: true, trustedProviders: ['google'] } })"
        )
        .is_empty());
    }

    #[test]
    fn allows_linking_disabled() {
        assert!(run("betterAuth({ accountLinking: { enabled: false } })").is_empty());
    }

    #[test]
    fn ignores_non_auth_files() {
        assert!(run("const x = 42").is_empty());
    }
}
