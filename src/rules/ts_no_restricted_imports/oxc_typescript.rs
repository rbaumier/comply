//! ts-no-restricted-imports OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Return true if `specifier` matches `pattern`. Supports:
///   - exact match: `lodash` matches `lodash`
///   - trailing `*`: `@banned/*` matches `@banned/foo`, `@banned/a/b`
fn specifier_matches(specifier: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        specifier.starts_with(prefix)
    } else {
        specifier == pattern
    }
}

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

        let patterns = ctx
            .config
            .string_list("ts-no-restricted-imports", "patterns", ctx.lang);
        if patterns.is_empty() {
            return;
        }

        let module_path = import.source.value.as_str();
        if module_path.is_empty() {
            return;
        }

        let Some(matched) = patterns.iter().find(|p| specifier_matches(module_path, p)) else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from `{module_path}` matches restricted pattern `{matched}`. See comply.toml for the restriction list."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn allows_any_import_when_no_restrictions_configured() {
        // Default config has no patterns set for this rule.
        let d = run_on("import type { Foo } from '@tanstack/react-table';");
        assert!(d.is_empty());
        let d = run_on("import { Foo } from 'bar';");
        assert!(d.is_empty());
        let d = run_on("import type Foo from './types';");
        assert!(d.is_empty());
    }
}
