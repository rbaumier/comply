//! max-union-size backend — flag union types with more than 5 members.

use crate::diagnostic::{Diagnostic, Severity};

/// Count the total leaf type members in a (possibly nested) union_type.
/// tree-sitter represents `A | B | C | D | E | F` as a left-recursive tree:
///   union_type(union_type(union_type(..., A, B), C), ..., F)
/// so direct child count only shows 2-3, not the actual member count.
fn count_union_members(node: tree_sitter::Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "|" {
            continue;
        }
        if child.kind() == "union_type" {
            count += count_union_members(child);
        } else {
            count += 1;
        }
    }
    count
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "union_type" {
        return;
    }

    // Only flag the outermost union_type (skip nested ones that are children of a union_type).
    if let Some(parent) = node.parent()
        && parent.kind() == "union_type"
    {
        return;
    }

    let max = ctx.config.threshold("max-union-size", "max");
    let count = count_union_members(node) as usize;

    if count > max {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "max-union-size".into(),
            message: format!(
                "Union type has {count} members (max: {max}) — consider extracting a type alias."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_large_union_in_type_alias() {
        let src = "type Status = A | B | C | D | E | F;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_large_union_in_annotation() {
        let src = "function foo(x: A | B | C | D | E | F) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_small_union() {
        let src = "type Status = A | B | C;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_five_members() {
        let src = "type X = A | B | C | D | E;";
        assert!(run_on(src).is_empty());
    }
}
