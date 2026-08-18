//! no-unknown-type-aliases oxc backend — flag an alias whose type resolves to
//! `unknown`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::unknown_type::resolves_to_unknown;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    /// The keyword has to be written somewhere in the file for an alias to
    /// resolve to it — alias resolution never leaves the file.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["unknown"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
            return;
        };
        // What `Box<T>` stands for depends on the argument, so a generic alias
        // is not the place to report.
        if alias.type_parameters.is_some() {
            return;
        }
        if !resolves_to_unknown(&alias.type_annotation, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, alias.id.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "this alias resolves to `unknown`, so every use site reads a name and \
                      gets the top type — name the type the value has, and narrow at the \
                      parsing boundary."
                .into(),
            severity: Severity::Error,
            span: None,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_bare_alias() {
        assert_eq!(run("type Json = unknown;").len(), 1);
    }

    #[test]
    fn flags_exported_alias() {
        assert_eq!(run("export type Json = unknown;").len(), 1);
    }

    #[test]
    fn flags_both_ends_of_a_chain() {
        assert_eq!(run("type A = unknown; type B = A;").len(), 2);
    }

    #[test]
    fn flags_parenthesized() {
        assert_eq!(run("type A = (unknown);").len(), 1);
    }

    #[test]
    fn flags_union_member() {
        assert_eq!(run("type A = string | unknown;").len(), 1);
    }

    #[test]
    fn flags_local_alias() {
        assert_eq!(run("function f() { type A = unknown; return 1 as A; }").len(), 1);
    }

    #[test]
    fn ignores_array_of_unknown() {
        assert!(run("type A = unknown[];").is_empty());
    }

    #[test]
    fn ignores_dictionary() {
        assert!(run("type A = Record<string, unknown>;").is_empty());
    }

    #[test]
    fn ignores_generic_alias() {
        assert!(run("type Box<T> = T;").is_empty());
    }

    #[test]
    fn ignores_generic_instantiation() {
        assert!(run("type Box<T> = T; type A = Box<unknown>;").is_empty());
    }

    /// A promise of `unknown` is a promise. Only the return contract reads
    /// through it, and `no-unknown-returns` owns that span.
    #[test]
    fn ignores_promise_of_unknown() {
        assert!(run("type A = Promise<unknown>;").is_empty());
    }

    #[test]
    fn ignores_cycle() {
        assert!(run("type A = B; type B = A;").is_empty());
    }

    #[test]
    fn ignores_self_reference() {
        assert!(run("type A = A;").is_empty());
    }

    #[test]
    fn ignores_property_of_unknown() {
        assert!(run("type A = { cause: unknown };").is_empty());
    }

    #[test]
    fn ignores_named_type() {
        assert!(run("type A = string;").is_empty());
    }
}
