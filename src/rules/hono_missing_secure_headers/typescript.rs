//! hono-missing-secure-headers backend — Hono app without secureHeaders().

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        // Only check Hono files.
        if !ctx.source.contains("from 'hono'") && !ctx.source.contains("from \"hono\"") {
            return Vec::new();
        }

        // Skip if secureHeaders is already imported.
        if ctx.source.contains("hono/secure-headers") {
            return Vec::new();
        }

        let source_bytes = ctx.source.as_bytes();
        let mut has_routes = false;
        let mut hono_line = None;

        walk_tree(tree, |node| {
            // Look for `new Hono()` for the report location.
            if node.kind() == "new_expression"
                && let Some(constructor) = node.child_by_field_name("constructor")
                    && constructor.utf8_text(source_bytes).unwrap_or("") == "Hono" && hono_line.is_none() {
                        hono_line = Some(node.start_position().row + 1);
                    }
            // Look for route method calls.
            if node.kind() == "call_expression"
                && let Some(callee) = node.child_by_field_name("function")
                    && callee.kind() == "member_expression"
                        && let Some(method) = callee.child_by_field_name("property") {
                            let name = method.utf8_text(source_bytes).unwrap_or("");
                            if matches!(name, "get" | "post" | "put" | "delete" | "patch") {
                                has_routes = true;
                            }
                        }
        });

        if !has_routes {
            return Vec::new();
        }

        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: hono_line.unwrap_or(1),
            column: 1,
            rule_id: "hono-missing-secure-headers".into(),
            message: "Hono app defines routes without `secureHeaders()` middleware — security headers are missing.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_hono_app_without_secure_headers() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.get('/', handler);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_hono_app_with_secure_headers() {
        let src = "import { Hono } from 'hono';\nimport { secureHeaders } from 'hono/secure-headers';\nconst app = new Hono();\napp.use(secureHeaders());\napp.get('/', handler);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.get('/', handler);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_hono_import_without_routes() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();";
        assert!(run_on(src).is_empty());
    }
}
