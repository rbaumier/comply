//! OXC backend for elysia-nodejs-adapter-required.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@elysiajs/node"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if ctx.source_contains("adapter:") {
            return;
        }

        if import.source.value.as_str() != "@elysiajs/node" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`@elysiajs/node` imported but no `adapter:` set on the Elysia constructor."
                .into(),
            severity: Severity::Error,
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
    fn flags_node_import_without_adapter() {
        let src = "import { node } from '@elysiajs/node';\nimport { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_node_with_adapter() {
        let src = "import { node } from '@elysiajs/node';\nimport { Elysia } from 'elysia';\nnew Elysia({ adapter: node() }).listen(3000);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_node_files() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
