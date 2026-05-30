//! better-auth-plugin-import-path — flag imports from the `better-auth/plugins`
//! barrel; require a specific plugin subpath instead.

use crate::diagnostic::{Diagnostic, Severity};

/// Plugins that have a dedicated subpath export in better-auth (e.g. `better-auth/plugins/two-factor`).
/// Plugins absent from this list (e.g. `openAPI`) are only available via the barrel and must not be flagged.
const PLUGINS_WITH_SUBPATHS: &[&str] = &[
    "twoFactor",
    "oAuthProxy",
    "username",
    "passkey",
    "magicLink",
    "anonymous",
    "bearer",
    "admin",
    "multiSession",
    "phoneNumber",
    "emailOtp",
    "oneTap",
    "organization",
    "mfa",
];

fn has_movable_import(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "import_clause" {
            let mut sub = child.walk();
            for c in child.named_children(&mut sub) {
                if c.kind() == "named_imports" {
                    let mut sub2 = c.walk();
                    for spec in c.named_children(&mut sub2) {
                        if spec.kind() == "import_specifier" {
                            if let Some(name_node) = spec.child_by_field_name("name") {
                                if let Ok(name) = name_node.utf8_text(source) {
                                    if PLUGINS_WITH_SUBPATHS.contains(&name) {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(source_node) = node.child_by_field_name("source") else { return };
    let import_path = source_node
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');

    if import_path != "better-auth/plugins" {
        return;
    }

    if !has_movable_import(node, source) {
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

    #[test]
    fn no_fp_for_barrel_only_plugin() {
        // openAPI has no specific subpath in better-auth — barrel import is the only option
        assert!(run("import { openAPI as betterAuthOpenApi } from 'better-auth/plugins'").is_empty());
    }
}
