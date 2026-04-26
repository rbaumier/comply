//! dockerfile-unique-stage-names tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

fn alias_of<'a>(from_node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    for j in 0..from_node.child_count() {
        let sub = from_node.child(j).unwrap();
        if sub.kind() == "image_alias" {
            return std::str::from_utf8(&source[sub.byte_range()])
                .ok()
                .map(str::trim);
        }
    }
    None
}

crate::ast_check! { on ["from_instruction"] => |node, source, ctx, diagnostics|
    let Some(alias) = alias_of(node, source) else { return; };
    let mut prev = node.prev_sibling();
    while let Some(sibling) = prev {
        if sibling.kind() == "from_instruction" {
            if let Some(other) = alias_of(sibling, source) {
                if other == alias {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: super::META.id.into(),
                        message: format!("Stage alias `{alias}` is already defined earlier."),
                        severity: Severity::Warning,
                        span: Some((node.byte_range().start, node.byte_range().len())),
                    });
                    return;
                }
            }
        }
        prev = sibling.prev_sibling();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_duplicate_alias() {
        let src = "FROM node:20 AS build\nRUN echo hi\nFROM alpine AS build\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unique_aliases() {
        let src = "FROM node:20 AS build\nFROM alpine AS runtime\n";
        assert!(run(src).is_empty());
    }
}
