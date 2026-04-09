//! no-type-encoded-names backend — flag identifiers encoding the variable's
//! type in the name: `strName`, `arrItems`, `boolReady`, `iCount`, `objUser`.
//!
//! Why: the TypeScript type system already tells you what `name` is —
//! adding `str` to the identifier is Hungarian notation, which was obsolete
//! the moment we got type checkers. Worse, the prefix lies when the type
//! changes: `strCount` becomes a number and nobody notices.
//!
//! Detection: walk identifier declarations and check if the name starts
//! with a type-prefix followed by a camelCase boundary (uppercase letter).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const TYPE_PREFIXES: &[&str] = &[
    "str", "arr", "obj", "num", "bool", "int", "fn", "func",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "identifier" {
                return;
            }
            if !is_declaration_site(node) {
                return;
            }
            let Ok(name) = node.utf8_text(source_bytes) else {
                return;
            };
            let Some(prefix) = matched_type_prefix(name) else {
                return;
            };
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-type-encoded-names".into(),
                message: format!(
                    "'{name}' encodes a type prefix '{prefix}' — Hungarian \
                     notation is obsolete. Remove the prefix; TypeScript's \
                     type checker already knows the type."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "variable_declarator" | "required_parameter" | "function_declaration"
    )
}

/// Return the type prefix matching `name` with a camelCase boundary, or None.
fn matched_type_prefix(name: &str) -> Option<&'static str> {
    let bytes = name.as_bytes();
    for &prefix in TYPE_PREFIXES {
        let plen = prefix.len();
        if bytes.len() <= plen {
            continue;
        }
        if !bytes[..plen].eq_ignore_ascii_case(prefix.as_bytes()) {
            continue;
        }
        // Next char must be uppercase (camelCase boundary).
        if bytes[plen].is_ascii_uppercase() {
            return Some(prefix);
        }
    }
    None
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
    fn flags_str_prefix() {
        assert_eq!(run_on("const strName = 'x';").len(), 1);
    }

    #[test]
    fn flags_arr_prefix() {
        assert_eq!(run_on("const arrItems = [];").len(), 1);
    }

    #[test]
    fn flags_bool_prefix() {
        assert_eq!(run_on("const boolReady = true;").len(), 1);
    }

    #[test]
    fn allows_descriptive_names() {
        assert!(run_on("const userName = 'x';").is_empty());
        assert!(run_on("const items = [];").is_empty());
        assert!(run_on("const isReady = true;").is_empty());
    }

    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // 'string' starts with 'str' but there's no camelCase boundary.
        assert!(run_on("const string = 'x';").is_empty());
        // 'array' starts with 'arr' but 'a' is lowercase after.
        assert!(run_on("const arrayList = 1;").is_empty());
    }
}
