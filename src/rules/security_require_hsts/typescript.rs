//! security-require-hsts backend —
//! Express app without HSTS header (no helmet, no Strict-Transport-Security).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if !ctx.source.contains("express") {
            return Vec::new();
        }
        // helmet() installs HSTS by default, so that's accepted.
        if ctx.source.contains("helmet(") {
            return Vec::new();
        }
        // Explicit HSTS header is also accepted.
        if ctx.source.contains("Strict-Transport-Security")
            || ctx.source.contains("strict-transport-security")
        {
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
                    "Express app has no HSTS header — install `helmet()` or set `Strict-Transport-Security`.".into(),
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
    fn flags_express_without_hsts() {
        let src = "import express from 'express';\nconst app = express();\napp.get('/', h);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_express_with_helmet() {
        let src = "import express from 'express';\nimport helmet from 'helmet';\nconst app = express();\napp.use(helmet());";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_express_with_explicit_hsts_header() {
        let src = "import express from 'express';\nconst app = express();\napp.use((req,res,next)=>{res.setHeader('Strict-Transport-Security','max-age=31536000');next();});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_express_files() {
        assert!(run("const x = 1;").is_empty());
    }
}
