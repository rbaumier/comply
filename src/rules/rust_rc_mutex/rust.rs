//! rust-rc-mutex backend.
//!
//! Detects the `Rc<Mutex<T>>` type pattern — a classic footgun where
//! the Mutex is dead weight. `Rc` isn't `Send`, so the value can
//! never cross a thread boundary, which means the Mutex protects
//! against nothing. Equivalent to `clippy::rc_mutex`, which is a
//! correctness lint but off by default because it's opinionated.
//!
//! The AST pattern we're looking for:
//!
//!     generic_type
//!       type: "Rc" (type_identifier)
//!       type_arguments
//!         generic_type
//!           type: "Mutex" (type_identifier)

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
            if node.kind() != "generic_type" {
                return;
            }
            if !type_name_is(&node, "Rc", source_bytes) {
                return;
            }
            let Some(args) = node.child_by_field_name("type_arguments") else {
                return;
            };
            if !first_type_arg_is(&args, "Mutex", source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-rc-mutex".into(),
                message: "`Rc<Mutex<T>>` — `Rc` is `!Send`, so the value can \
                          never cross threads and the `Mutex` protects against \
                          nothing. Use `Rc<RefCell<T>>` for single-threaded \
                          interior mutability, or `Arc<Mutex<T>>` if you \
                          actually share across threads."
                    .into(),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

/// True if `generic.child_by_field_name("type")` points to a
/// `type_identifier` with the given name (accepts bare `Rc` or
/// qualified `std::rc::Rc`).
fn type_name_is(generic: &tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let Some(type_node) = generic.child_by_field_name("type") else {
        return false;
    };
    let Ok(text) = type_node.utf8_text(source) else {
        return false;
    };
    text == name || text.ends_with(&format!("::{name}"))
}

/// True if the first type argument inside `type_arguments` is a
/// `generic_type` whose base type identifier matches `name`.
fn first_type_arg_is(
    type_arguments: &tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    let mut cursor = type_arguments.walk();
    for child in type_arguments.named_children(&mut cursor) {
        if child.kind() == "generic_type" && type_name_is(&child, name, source) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_rc_mutex_type() {
        let source = "fn f() -> Rc<Mutex<i32>> { todo!() }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_rc_mutex_in_struct_field() {
        let source = "struct S { inner: Rc<Mutex<Vec<u8>>> }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_rc_refcell() {
        let source = "fn f() -> Rc<RefCell<i32>> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_arc_mutex() {
        let source = "fn f() -> Arc<Mutex<i32>> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_rc() {
        let source = "fn f() -> Rc<i32> { todo!() }";
        assert!(run_on(source).is_empty());
    }
}
