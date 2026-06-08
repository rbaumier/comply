//! no-useless-iterator-to-array AST backend — flag `.toArray()` invoked
//! in a context that already accepts an iterable (for-of, spread,
//! collection constructor, `Array.from`, `Object.fromEntries`, `yield*`).
//!
//! Strategy: walk every `call_expression` whose function is a
//! `member_expression` with property `toArray` and zero arguments. Then
//! climb the parent chain to classify the surrounding iterable context.

use crate::diagnostic::{Diagnostic, Severity};

const COLLECTIONS: &[&str] = &["Set", "Map", "WeakSet", "WeakMap"];

/// Returns true if `node` is `<x>.toArray()` with no arguments.
fn is_to_array_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    if std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("") != "toArray" {
        return false;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    args.named_children(&mut cursor).next().is_none()
}

/// Identify the iterable context surrounding `call`. Returns `(message,
/// anchor_node)` where `anchor_node` is the outer call/expression to
/// highlight.
fn classify_context<'a>(
    call: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<(&'static str, tree_sitter::Node<'a>)> {
    let parent = call.parent()?;

    match parent.kind() {
        // `for (const x of <call>)` — the call is the right-hand side of
        // the `for_in_statement` (TS grammar uses for_in_statement for
        // both for-in and for-of).
        "for_in_statement" => {
            // Confirm it's a for-of by looking for the `of` keyword among
            // the children.
            let mut cursor = parent.walk();
            let is_for_of = parent.children(&mut cursor).any(|c| c.kind() == "of");
            if is_for_of {
                return Some((
                    "`for...of` can iterate over an iterable, `.toArray()` is unnecessary.",
                    parent,
                ));
            }
            None
        }
        // `[...x.toArray()]` and `f(...x.toArray())` — spread element wraps the call.
        "spread_element" => Some((
            "Spread works on iterables, `.toArray()` is unnecessary.",
            parent,
        )),
        // `yield* x.toArray()` — yield_expression with `*`.
        "yield_expression" => {
            let mut cursor = parent.walk();
            let has_star = parent.children(&mut cursor).any(|c| c.kind() == "*");
            if has_star {
                return Some((
                    "`yield*` can delegate to an iterable, `.toArray()` is unnecessary.",
                    parent,
                ));
            }
            None
        }
        // Direct argument to `new Set(...)`, `Array.from(...)`,
        // `Object.fromEntries(...)`. The parent is `arguments`, which in
        // turn is a child of the outer call/new expression.
        "arguments" => {
            let outer = parent.parent()?;
            match outer.kind() {
                "new_expression" => {
                    let ctor = outer.child_by_field_name("constructor")?;
                    let name = std::str::from_utf8(&source[ctor.byte_range()]).unwrap_or("");
                    if COLLECTIONS.contains(&name) {
                        return Some((
                            "Collection constructor accepts an iterable, `.toArray()` is unnecessary.",
                            outer,
                        ));
                    }
                    None
                }
                "call_expression" => {
                    let func = outer.child_by_field_name("function")?;
                    let callee = std::str::from_utf8(&source[func.byte_range()]).unwrap_or("");
                    if callee == "Array.from" {
                        return Some((
                            "`Array.from()` accepts an iterable, `.toArray()` is unnecessary.",
                            outer,
                        ));
                    }
                    if callee == "Object.fromEntries" {
                        return Some((
                            "`Object.fromEntries()` accepts an iterable, `.toArray()` is unnecessary.",
                            outer,
                        ));
                    }
                    None
                }
                _ => None,
            }
        }
        _ => None,
    }
}

crate::ast_check! { prefilter = ["toArray"] => |node, source, ctx, diagnostics|
    if !is_to_array_call(node, source) {
        return;
    }
    let Some((msg, anchor)) = classify_context(node, source) else { return };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &anchor,
        super::META.id,
        msg.into(),
        Severity::Warning,
    ));
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
    fn flags_for_of_to_array() {
        let d = run_on("for (const x of iter.toArray()) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("for...of"));
    }

    #[test]
    fn flags_spread_to_array() {
        let d = run_on("const arr = [...iter.toArray()];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Spread"));
    }

    #[test]
    fn flags_new_set_to_array() {
        let d = run_on("const s = new Set(iter.toArray());");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Collection"));
    }

    #[test]
    fn flags_array_from_to_array() {
        let d = run_on("const a = Array.from(iter.toArray());");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn allows_standalone_to_array() {
        assert!(run_on("const arr = iter.toArray();").is_empty());
    }

    #[test]
    fn allows_non_to_array_method() {
        assert!(run_on("for (const x of iter.values()) {}").is_empty());
    }
}
