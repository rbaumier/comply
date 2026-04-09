//! no-json-parse-cast backend — reject `JSON.parse(x) as T`.
//!
//! Why: `JSON.parse` returns `any` (morally `unknown`), and immediately
//! casting the result to a typed shape is a lie. If the JSON doesn't match
//! the type, the lie crashes far from the origin. The correct approach is
//! to validate with a type guard or Zod schema at the boundary.
//!
//! Detection: walk `as_expression` nodes whose value side is a
//! `call_expression` whose callee is `JSON.parse`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "as_expression" {
                return;
            }
            let Some(inner) = node.named_child(0) else {
                return;
            };
            if inner.kind() != "call_expression" {
                return;
            }
            let Some(callee) = inner.child_by_field_name("function") else {
                return;
            };
            let Ok(callee_text) = callee.utf8_text(source_bytes) else {
                return;
            };
            if callee_text != "JSON.parse" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-json-parse-cast".into(),
                message: "Casting `JSON.parse(...) as T` is a lie — the \
                          runtime shape may not match T. Validate with a \
                          Zod schema (`Schema.safeParse(JSON.parse(raw))`) \
                          or a type guard function that inspects the value."
                    .into(),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_json_parse_as_type() {
        assert_eq!(run_on("const u = JSON.parse(raw) as User;").len(), 1);
    }

    #[test]
    fn allows_json_parse_with_schema() {
        assert!(
            run_on("const u = UserSchema.parse(JSON.parse(raw));").is_empty()
        );
    }

    #[test]
    fn allows_other_cast() {
        assert!(run_on("const u = value as User;").is_empty());
    }

    #[test]
    fn does_not_flag_other_function_call_cast() {
        assert!(run_on("const u = getRaw() as User;").is_empty());
    }
}
