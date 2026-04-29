//! elysia-bearer-missing-www-auth backend — flag bearer 401/400 without WWW-Authenticate.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> { Some(&["WWW-Authenticate"]) }

    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }
        if ctx.source.contains("WWW-Authenticate") {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("status(401") || line.contains("status(400") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "elysia-bearer-missing-www-auth".into(),
                    message: "Bearer auth 401/400 response without `WWW-Authenticate` header (RFC 6750).".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_401_without_www_authenticate() {
        let src = "import { bearer } from '@elysiajs/bearer';\nreturn status(401, 'unauthorized');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_401_with_www_authenticate() {
        let src = "import { bearer } from '@elysiajs/bearer';\nset.headers['WWW-Authenticate'] = 'Bearer realm=\"api\"';\nreturn status(401, 'unauthorized');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "return status(401, 'unauthorized');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
