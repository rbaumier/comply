//! better-auth-plugin-import-path — flag imports from the `better-auth/plugins`
//! barrel; require a specific plugin subpath instead.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(source_node) = node.child_by_field_name("source") else { return };
    let import_path = source_node
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');

    if import_path != "better-auth/plugins" {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &source_node,
        super::META.id,
        "Import from `better-auth/plugins` barrel prevents tree-shaking — use a specific path like `better-auth/plugins/two-factor`.".into(),
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
    fn flags_generic_barrel_import() {
        assert_eq!(
            run("import { twoFactor } from \"better-auth/plugins\"").len(),
            1
        );
    }

    #[test]
    fn flags_single_quote_barrel() {
        assert_eq!(
            run("import { oAuthProxy } from 'better-auth/plugins'").len(),
            1
        );
    }

    #[test]
    fn allows_specific_plugin_path() {
        assert!(run("import { twoFactor } from \"better-auth/plugins/two-factor\"").is_empty());
    }

    #[test]
    fn allows_core_import() {
        assert!(run("import { betterAuth } from \"better-auth\"").is_empty());
    }
}
