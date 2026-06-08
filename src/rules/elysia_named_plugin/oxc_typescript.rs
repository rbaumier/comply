//! elysia-named-plugin — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let Expression::Identifier(ident) = &new_expr.callee else { return };
        if ident.name.as_str() != "Elysia" {
            return;
        }

        // Walk ancestors to see if this is part of an export.
        let mut exported = false;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::ExportDefaultDeclaration(_) | AstKind::ExportNamedDeclaration(_) => {
                    exported = true;
                    break;
                }
                _ => {}
            }
        }
        if !exported {
            return;
        }

        // Check arguments for `name:` field.
        let args_start = new_expr.span.start as usize;
        let args_end = new_expr.span.end as usize;
        let args_text = ctx.source.get(args_start..args_end).unwrap_or("");
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("name:'") || norm.contains("name:\"") || norm.contains("name:`") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Exported Elysia plugin has no `name` \u{2014} pass `new Elysia({ name: '...' })` for deduplication and clearer error traces.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_exported_unnamed_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia();";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_exported_options_without_name() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia({ prefix: '/v1' });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_named_plugin() {
        let src =
            "import { Elysia } from 'elysia';\nexport const plugin = new Elysia({ name: 'auth' });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_non_exported_app() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia();";
        assert!(run_on(src).is_empty());
    }
}
