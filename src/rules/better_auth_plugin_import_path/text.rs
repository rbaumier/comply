use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.starts_with("import") {
                continue;
            }
            if t.contains("\"better-auth/plugins\"") || t.contains("'better-auth/plugins'") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-plugin-import-path".into(),
                    message: "Import from `better-auth/plugins` barrel prevents tree-shaking — use a specific path like `better-auth/plugins/two-factor`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
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
