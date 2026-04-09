//! no-enum backend — flag every `enum` declaration in TypeScript.
//!
//! Why: TypeScript enums emit runtime code, have surprising numeric
//! reverse-mappings, and don't narrow as cleanly as discriminated unions.
//! The idiomatic replacement for string enums is
//! `const X = { a: 'a', b: 'b' } as const satisfies Record<string, string>`,
//! and for tagged data a discriminated union with a `type` field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "enum_declaration" {
                return;
            }
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
                .unwrap_or("<enum>");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-enum".into(),
                message: format!(
                    "Enum '{name}' — replace with `const {name} = {{ ... }} as const \
                     satisfies Record<string, string>` (for config) or a \
                     discriminated union with a `type` field (for tagged data). \
                     Enums emit runtime code and don't narrow cleanly."
                ),
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
    fn flags_enum_declaration() {
        assert_eq!(run_on("enum Status { Active, Inactive }").len(), 1);
    }

    #[test]
    fn flags_const_enum() {
        assert_eq!(run_on("const enum Role { Admin, User }").len(), 1);
    }

    #[test]
    fn allows_as_const_satisfies() {
        let source =
            "const STATUS = { active: 'active', inactive: 'inactive' } as const satisfies Record<string, string>;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_discriminated_union() {
        let source = "type Status = { type: 'active' } | { type: 'inactive' };";
        assert!(run_on(source).is_empty());
    }
}
