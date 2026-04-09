//! law-of-demeter backend — max one dot deep on dependency chains.
//!
//! Why: `order.getCustomer().getAddress().getCity()` couples the caller to
//! the entire object graph. If `Address` restructures, every caller breaks.
//! Expose a direct accessor (`order.shippingCity()`) instead.
//!
//! Detection: walk `member_expression` nodes and count the depth of
//! chained member access. A chain like `a.b.c.d` has depth 3. We flag
//! anything with depth ≥ 3, which allows `a.b.c` (one direct access to
//! an owned child) but not `a.b.c.d`.
//!
//! Exceptions:
//! - `this.*.*.*` is allowed — accessing own fields is not coupling.
//! - Chains where every segment is an UPPERCASE_CONSTANT (e.g. `CONFIG.DB.URL`)
//!   are allowed — that's module-level config, not object traversal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const MAX_DEPTH: usize = 3;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "member_expression" {
                return;
            }
            // Skip inner member expressions — only analyze the outermost.
            if node
                .parent()
                .is_some_and(|p| p.kind() == "member_expression")
            {
                return;
            }
            let depth = chain_depth(node);
            if depth < MAX_DEPTH {
                return;
            }
            if is_self_chain(node, source_bytes) || is_constant_chain(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "law-of-demeter".into(),
                message: format!(
                    "Chained property access {depth} levels deep — Law of \
                     Demeter violation. Add a direct accessor on the \
                     immediate dependency instead of reaching through."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// Count how many `.foo` segments the chain has.
fn chain_depth(node: tree_sitter::Node) -> usize {
    let mut depth = 0;
    let mut current = node;
    while current.kind() == "member_expression" {
        depth += 1;
        match current.child_by_field_name("object") {
            Some(obj) => current = obj,
            None => break,
        }
    }
    depth
}

/// True if the chain root is `this` — own-field access is exempt.
fn is_self_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while current.kind() == "member_expression" {
        match current.child_by_field_name("object") {
            Some(obj) => current = obj,
            None => return false,
        }
    }
    current
        .utf8_text(source)
        .is_ok_and(|t| t == "this" || t == "self")
}

/// True if every segment of the chain is UPPERCASE_SNAKE_CASE — module config.
fn is_constant_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while current.kind() == "member_expression" {
        let Some(prop) = current.child_by_field_name("property") else {
            return false;
        };
        let Ok(name) = prop.utf8_text(source) else {
            return false;
        };
        if !is_screaming_snake_case(name) {
            return false;
        }
        match current.child_by_field_name("object") {
            Some(obj) => current = obj,
            None => break,
        }
    }
    current
        .utf8_text(source)
        .is_ok_and(is_screaming_snake_case)
}

fn is_screaming_snake_case(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
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
    fn flags_three_level_chain() {
        assert_eq!(run_on("const x = order.customer.address.city;").len(), 1);
    }

    #[test]
    fn allows_two_level_chain() {
        assert!(run_on("const x = order.customer;").is_empty());
        assert!(run_on("const x = order.customer.name;").is_empty());
    }

    #[test]
    fn allows_this_chain() {
        assert!(run_on("class C { f() { return this.a.b.c.d; } }").is_empty());
    }

    #[test]
    fn allows_constant_config_chain() {
        assert!(run_on("const u = CONFIG.DATABASE.URL.HOSTNAME;").is_empty());
    }
}
