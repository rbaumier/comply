//! Iterative tree-sitter walker — visits every node without recursion.
//!
//! Why iterative: a recursive walker would call itself once per nesting level
//! in the AST. Adversarial input (e.g. `a?b?c?...:0:0:0:0`) can produce trees
//! tens of thousands of levels deep, blowing the Rust stack and crashing
//! comply on the user's source. The iterative version uses tree-sitter's
//! built-in cursor with bounded heap state — depth is no longer a concern.
//!
//! Centralizing the walk also removes ~80 lines of duplicated cursor mechanics
//! across the rule files.

/// Visit every "well-formed" node in the tree, calling `visit` once per node.
///
/// Skips entire subtrees rooted at ERROR or MISSING nodes — without this skip,
/// tree-sitter's syntax recovery emits phantom `function_declaration`,
/// `throw_statement`, etc. children inside garbage subtrees, producing
/// nonsense diagnostics on files with parse errors.
///
/// The visitor receives the current node by value (cheap — `Node` is just a
/// pointer pair) and decides whether to record a diagnostic for it.
pub fn walk_tree<F>(tree: &tree_sitter::Tree, mut visit: F)
where
    F: FnMut(tree_sitter::Node),
{
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();

        // Skip the entire subtree if this node is part of a parse-error region.
        // We don't visit it, and we don't descend into it.
        if node.is_error() || node.is_missing() {
            if !advance_to_next_sibling(&mut cursor) {
                return;
            }
            continue;
        }

        visit(node);

        if cursor.goto_first_child() {
            continue;
        }

        if !advance_to_next_sibling(&mut cursor) {
            return;
        }
    }
}

/// Advance the cursor to the next sibling, walking up the tree if needed.
/// Returns false if we walked back to the root with no more siblings — the
/// caller should stop iterating.
fn advance_to_next_sibling(cursor: &mut tree_sitter::TreeCursor) -> bool {
    loop {
        if cursor.goto_next_sibling() {
            return true;
        }
        if !cursor.goto_parent() {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn visits_every_node_in_simple_tree() {
        let tree = parse("const x = 1;");
        let mut count = 0;
        walk_tree(&tree, |_| count += 1);
        assert!(count > 0);
    }

    #[test]
    fn visits_nested_nodes() {
        let tree = parse("function f() { return a + b; }");
        let mut found_return = false;
        walk_tree(&tree, |node| {
            if node.kind() == "return_statement" {
                found_return = true;
            }
        });
        assert!(found_return);
    }

    #[test]
    fn handles_empty_source() {
        let tree = parse("");
        let mut count = 0;
        walk_tree(&tree, |_| count += 1);
        // Empty program still has a root program node.
        assert!(count >= 1);
    }

    #[test]
    fn skips_subtrees_under_error_nodes() {
        // Truncated source — tree-sitter recovers with ERROR nodes.
        let tree = parse("function f() { const x =");
        let mut throw_count = 0;
        walk_tree(&tree, |node| {
            if node.kind() == "throw_statement" {
                throw_count += 1;
            }
        });
        // Even if recovery synthesizes anything weird, we shouldn't hallucinate throws.
        assert_eq!(throw_count, 0);
    }

    #[test]
    fn handles_deeply_nested_input_without_overflow() {
        // Adversarial: 1000 nested ternaries — would blow recursive walker.
        let mut source = String::from("const x = ");
        for _ in 0..1000 {
            source.push_str("a?");
        }
        source.push('1');
        for _ in 0..1000 {
            source.push_str(":0");
        }
        source.push(';');
        let tree = parse(&source);
        let mut count = 0;
        walk_tree(&tree, |_| count += 1);
        assert!(count > 1000);
    }
}
