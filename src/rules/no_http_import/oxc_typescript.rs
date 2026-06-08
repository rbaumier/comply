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
        Some(&["http://"])
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

        let specifier = import.source.value.as_str();
        if !specifier.starts_with("http://") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Insecure `http://` import `{specifier}` — use `https://` instead."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::no_http_import::oxc_typescript::Check;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_http_import() {
        let diags = run("import { something } from 'http://cdn.example.com/lib.js';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("http://"));
    }


    #[test]
    fn allows_https_import() {
        assert!(run("import { something } from 'https://cdn.example.com/lib.js';").is_empty());
    }


    #[test]
    fn allows_local_import() {
        assert!(run("import { foo } from './foo';").is_empty());
    }


    #[test]
    fn allows_npm_import() {
        assert!(run("import express from 'express';").is_empty());
    }


    #[test]
    fn flags_http_side_effect_import() {
        let diags = run("import 'http://cdn.example.com/polyfill.js';");
        assert_eq!(diags.len(), 1);
    }
}
