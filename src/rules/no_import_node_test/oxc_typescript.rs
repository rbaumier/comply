//! no-import-node-test oxc backend — flag `import ... from 'node:test'`.

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
        Some(&["node:test"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        if import.source.value.as_str() != "node:test" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Importing from `node:test` mixes test runners; use vitest/jest APIs instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_default_node_test_import() {
        let d = run_on("import test from 'node:test';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("node:test"));
    }


    #[test]
    fn flags_named_node_test_import() {
        let d = run_on("import { describe, it } from 'node:test';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_double_quoted_node_test_import() {
        let d = run_on("import { test } from \"node:test\";");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_vitest_import() {
        assert!(run_on("import { describe, it } from 'vitest';").is_empty());
    }


    #[test]
    fn allows_jest_import() {
        assert!(run_on("import { jest } from '@jest/globals';").is_empty());
    }


    #[test]
    fn allows_other_node_builtin() {
        assert!(run_on("import { readFile } from 'node:fs';").is_empty());
    }
}
