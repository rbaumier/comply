//! prefer-modern-dom-apis oxc backend — flag legacy DOM mutation methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const PATTERNS: &[(&str, &str)] = &[
    (
        "insertBefore",
        "Prefer `ref.before(newNode)` over `parent.insertBefore(newNode, ref)`.",
    ),
    (
        "replaceChild",
        "Prefer `old.replaceWith(newNode)` over `parent.replaceChild(newNode, old)`.",
    ),
];

/// Import sources whose tree nodes expose `insertBefore` / `replaceChild` for
/// non-DOM tree manipulation. These methods are not the browser `Node` API the
/// rule targets — PostCSS's `Container.insertBefore(ref, new)` even reverses
/// the argument order — so rewriting them to `.before()` / `.replaceWith()`
/// would be wrong. When a file imports from one of these, its `insertBefore` /
/// `replaceChild` receivers cannot be DOM nodes, so the rule stays silent.
const NON_DOM_TREE_LIBRARIES: &[&str] = &["postcss", "cheerio"];

/// True when the file imports from a library whose AST nodes expose look-alike
/// `insertBefore` / `replaceChild` methods. Matches the quoted import-source
/// form (`from "postcss"` / `from 'postcss'`) so a short token cannot match an
/// unrelated substring.
fn imports_non_dom_tree_library(ctx: &CheckCtx) -> bool {
    NON_DOM_TREE_LIBRARIES.iter().any(|source| {
        ctx.source_contains(&format!("from \"{source}\""))
            || ctx.source_contains(&format!("from '{source}'"))
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["insertBefore", "replaceChild"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let name = member.property.name.as_str();
        let Some((_, message)) = PATTERNS.iter().find(|(p, _)| *p == name) else {
            return;
        };

        if imports_non_dom_tree_library(ctx) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: (*message).into(),
            severity: Severity::Warning,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_insert_before() {
        let d = run("parent.insertBefore(newNode, refNode);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("before"));
    }

    #[test]
    fn flags_replace_child() {
        let d = run("parent.replaceChild(newEl, oldEl);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("replaceWith"));
    }

    #[test]
    fn allows_postcss_container_insert_before_issue_3335() {
        // Regression for rbaumier/comply#3335 — PostCSS's
        // `Container.insertBefore(ref, new)` is a non-DOM AST API with reversed
        // argument order, not the browser `Node.insertBefore`. Files importing
        // from `postcss` must not be flagged.
        let src = "import postcss, { type Root } from \"postcss\";\n\
                   function applyVarsToRoot(ast: Root): void {\n\
                     ast.insertBefore(variantNode, postcss.comment({ text: \"---break---\" }));\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_postcss_replace_child() {
        let src = "import { Root } from 'postcss';\n\
                   container.replaceChild(oldNode, newNode);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_dom_insert_before_without_non_dom_import() {
        let src = "import { foo } from './utils';\n\
                   parent.insertBefore(newNode, refNode);";
        assert_eq!(run(src).len(), 1);
    }
}
