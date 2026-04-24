//! security-require-helmet backend — Express app without `helmet()` middleware.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        // Only check files that import or create Express apps.
        if !ctx.source.contains("express") {
            return Vec::new();
        }
        // If helmet() is registered anywhere in this file, we're fine.
        if ctx.source.contains("helmet(") {
            return Vec::new();
        }

        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if !diagnostics.is_empty() {
                return;
            }
            if node.kind() != "call_expression" {
                return;
            }
            let Some(name) = crate::rules::call_expression::call_function_name(node, source_bytes)
            else {
                return;
            };
            if name == "express" {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "Express app created without `helmet()` middleware — default security headers are missing.".into(),
                    Severity::Error,
                ));
            }
        });

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_express_without_helmet() {
        let src = "import express from 'express';\nconst app = express();\napp.get('/', handler);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_express_with_helmet() {
        let src = "import express from 'express';\nimport helmet from 'helmet';\nconst app = express();\napp.use(helmet());";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_files_without_express() {
        assert!(run("const x = 1;").is_empty());
    }
}
