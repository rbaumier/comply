//! no-full-import OXC backend — flag whole-library imports from utility
//! packages known to break tree-shaking.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ImportDeclarationSpecifier;
use std::sync::Arc;

pub struct Check;

const HEAVY_LIBS: &[&str] = &["lodash", "underscore", "ramda"];

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
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        let module = import.source.value.as_str();
        if !HEAVY_LIBS.contains(&module) {
            return;
        }

        let Some(ref specifiers) = import.specifiers else {
            // Side-effect import `import 'lodash';` — no specifiers to flag.
            return;
        };

        let mut has_default = false;
        let mut has_namespace = false;
        for spec in specifiers {
            match spec {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => has_default = true,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => has_namespace = true,
                _ => {}
            }
        }
        if !has_default && !has_namespace {
            return;
        }

        let form = if has_namespace { "namespace" } else { "default" };
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Avoid {form} import of the whole `{module}` library — import the specific function \
                 (e.g. `{module}/debounce` or a named import) to keep the bundle small."
            ),
            severity: Severity::Warning,
            span: Some((
                import.span.start as usize,
                (import.span.end - import.span.start) as usize,
            )),
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_default_lodash_import() {
        let d = run_on("import _ from 'lodash';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("lodash"));
    }

    #[test]
    fn flags_namespace_lodash_import() {
        let d = run_on("import * as _ from 'lodash';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("namespace"));
    }

    #[test]
    fn flags_default_underscore_import() {
        let d = run_on("import _ from 'underscore';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_default_ramda_import() {
        let d = run_on("import R from 'ramda';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_named_lodash_import() {
        assert!(run_on("import { debounce } from 'lodash';").is_empty());
    }

    #[test]
    fn allows_sub_path_import() {
        assert!(run_on("import debounce from 'lodash/debounce';").is_empty());
    }

    #[test]
    fn allows_unrelated_library() {
        assert!(run_on("import React from 'react';").is_empty());
    }
}
