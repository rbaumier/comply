//! better-auth-plugin-import-path oxc backend — flag barrel imports from `better-auth/plugins`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Plugins that have a dedicated subpath export in better-auth (e.g. `better-auth/plugins/two-factor`).
/// Imports of these names from the barrel can be moved to a specific path.
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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        if import.source.value.as_str() != "better-auth/plugins" {
            return;
        }

        let Some(specifiers) = &import.specifiers else {
            return;
        };
        let has_movable = specifiers.iter().any(|spec| {
            if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec {
                PLUGINS_WITH_SUBPATHS.contains(&named.imported.name().as_str())
            } else {
                false
            }
        });
        if !has_movable {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Import from `better-auth/plugins` barrel prevents tree-shaking — use a specific path like `better-auth/plugins/two-factor`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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
