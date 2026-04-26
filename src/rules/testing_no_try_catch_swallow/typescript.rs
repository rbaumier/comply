//! testing-no-try-catch-swallow backend — flag `try { ... } catch { }`
//! where the catch body is empty, inside a `test()` / `it()` callback.
//!
//! Why: tests use try/catch/empty to paper over "this sometimes throws,
//! ignore it". That's the opposite of testing: the assertion disappears.
//! Use `expect(...).toThrow(...)` or `expect(promise).rejects.toThrow(...)`.

use crate::diagnostic::{Diagnostic, Severity};

fn catch_body_is_empty(catch_clause: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = catch_clause.child_by_field_name("body") else { return false; };
    if body.kind() != "statement_block" { return false; }
    if body.named_child_count() != 0 { return false; }
    // If the block text contains any non-whitespace-non-brace char, treat as non-empty
    // (catches `{ /* tolerated */ }`). Otherwise it's truly empty.
    let text = body.utf8_text(source).unwrap_or("");
    text.chars().all(|c| c.is_whitespace() || c == '{' || c == '}')
}

fn inside_test_callback(mut node: tree_sitter::Node, source: &[u8]) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "call_expression"
            && let Some(func) = parent.child_by_field_name("function")
                && func.kind() == "identifier" {
                    let n = func.utf8_text(source).unwrap_or("");
                    if matches!(n, "test" | "it") { return true; }
                }
        node = parent;
    }
    false
}

crate::ast_check! { on ["try_statement"] => |node, source, ctx, diagnostics|
    // Find the catch_clause child.
    let mut catch_clause: Option<tree_sitter::Node> = None;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "catch_clause" {
            catch_clause = Some(child);
            break;
        }
    }
    let Some(cc) = catch_clause else { return; };
    if !catch_body_is_empty(cc, source) { return; }
    if !inside_test_callback(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Empty catch in a test masks the errors the test is meant to surface — assert with expect(...).toThrow(...) instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_empty_catch_in_test() {
        let src = "test('a', () => { try { doThing(); } catch { } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_catch_with_param_in_it() {
        let src = "it('a', () => { try { doThing(); } catch (e) { } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_catch_that_asserts_in_test() {
        let src = "test('a', () => {\n\
                     try { doThing(); } catch (e) { expect(e).toBeInstanceOf(Error); }\n\
                   });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_empty_catch_outside_test() {
        let src = "function helper() { try { doThing(); } catch { } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_finally_only_without_empty_catch() {
        let src = "test('a', () => { try { doThing(); } finally { cleanup(); } });";
        assert!(run(src).is_empty());
    }
}
