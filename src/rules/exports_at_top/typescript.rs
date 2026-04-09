//! exports-at-top backend — all exports must appear before the first
//! non-export declaration.
//!
//! Why: exports are the public API of a module. Putting them at the top
//! lets a reader skim the file and immediately see what's exposed without
//! wading through private helpers. When exports are interleaved with
//! private code, the reader has to scan the whole file to find them.
//!
//! Detection: walk the program's top-level children in order. Once we see
//! a non-export declaration, any later `export_statement` (or `export const`
//! style `export` prefix on a lexical declaration) is flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Child node kinds that are considered "non-export declarations" — when
/// one of these appears before any export, subsequent exports are violations.
const NON_EXPORT_DECLS: &[&str] = &[
    "lexical_declaration",
    "variable_declaration",
    "function_declaration",
    "class_declaration",
    "type_alias_declaration",
    "interface_declaration",
    "enum_declaration",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let root = tree.root_node();
        let mut seen_non_export = false;
        let mut diagnostics = Vec::new();

        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            let kind = child.kind();
            if NON_EXPORT_DECLS.contains(&kind) {
                seen_non_export = true;
                continue;
            }
            if kind == "export_statement" && seen_non_export {
                let pos = child.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "exports-at-top".into(),
                    message: "Export declared after a non-export — move all \
                              exports to the top of the file so readers see \
                              the public API at a glance."
                        .into(),
                    severity: Severity::Warning,
                });
            }
        }
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
            &CheckCtx {
                path: Path::new("t.ts"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn allows_exports_at_top_then_helpers() {
        let source = "export const foo = 1;\nexport const bar = 2;\nconst helper = 3;\n";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_export_after_declaration() {
        let source = "const helper = 1;\nexport const foo = 2;\n";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_export_after_function() {
        let source = "function helper() {}\nexport const foo = 2;\n";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_only_exports() {
        assert!(run_on("export const a = 1;\nexport const b = 2;\n").is_empty());
    }

    #[test]
    fn allows_only_non_exports() {
        assert!(run_on("const a = 1;\nconst b = 2;\n").is_empty());
    }
}
