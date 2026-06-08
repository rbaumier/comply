//! no-useless-intersection AST backend — intersection containing `unknown` or `never`.
//!
//! Walks `intersection_type` nodes and flags those whose direct members include
//! a `predefined_type` matching `unknown` or `never`. tree-sitter parses
//! `A & B & C` as a left-recursive `intersection_type(intersection_type(A, B), C)`,
//! so we only need to inspect each `intersection_type` node's own children
//! (any nested intersection is already its own visit).
//!
//! `& any` is intentionally excluded: removing it changes `Foo & any` (= `any`)
//! into `Foo`, which is a stricter type — not an equivalent simplification.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["intersection_type"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    let mut found = false;
    for child in node.children(&mut cursor) {
        if child.kind() != "predefined_type" {
            continue;
        }
        let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
        if text == "unknown" || text == "never" {
            found = true;
            break;
        }
    }

    if !found {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-useless-intersection".into(),
        message: "Intersection with `unknown` or `never` is useless — simplify it.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_intersection_with_unknown() {
        assert_eq!(run_on("type X = Foo & unknown;").len(), 1);
    }

    #[test]
    fn flags_unknown_on_left() {
        assert_eq!(run_on("type X = unknown & Foo;").len(), 1);
    }

    #[test]
    fn flags_intersection_with_never() {
        assert_eq!(run_on("type X = Foo & never;").len(), 1);
    }

    #[test]
    fn allows_intersection_with_any() {
        // `Foo & any` = `any`; removing `& any` would narrow the type to `Foo`.
        assert!(run_on("type X = Foo & any;").is_empty());
    }

    #[test]
    fn allows_any_on_left() {
        assert!(run_on("type X = any & Foo;").is_empty());
    }

    #[test]
    fn allows_normal_intersection() {
        assert!(run_on("type X = Foo & Bar;").is_empty());
    }

    #[test]
    fn no_false_positive_on_any_prefix() {
        assert!(run_on("type X = anything & Foo;").is_empty());
    }
}
