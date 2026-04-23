//! no-identical-title backend — flag repeated describe/test/it titles
//! within the same lexical scope.
//!
//! Why: two `it('handles empty input', …)` at the same level make CI
//! output ambiguous — when one fails, you can't tell which assertion
//! ran. Jest/Vitest don't dedupe titles, they just report both with the
//! same label, so failures become untraceable.
//!
//! Scope = one describe callback body (or the program root). Identical
//! titles in *different* describes are fine; titles on different
//! constructs (e.g. one `describe('x')` and one `test('x')`) are also
//! fine — only same-construct + same-title + same-scope collisions are
//! flagged.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

/// Which test constructs we track. `.only` / `.skip` variants reuse the
/// base kind (`describe.only('x', …)` collides with `describe('x', …)`
/// because Jest treats them as the same suite title).
const TEST_BASES: &[&str] = &["describe", "test", "it"];

/// Classify a call expression as one of our tracked test constructs,
/// returning `(kind, title)` when the first argument is a string
/// literal and the callee resolves to a recognised base. Returns
/// `None` for non-matching calls (including dynamic titles).
fn classify_test_call(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<(&'static str, String)> {
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node.child_by_field_name("function")?;
    let kind = match callee.kind() {
        "identifier" => {
            let name = callee.utf8_text(source).ok()?;
            TEST_BASES.iter().copied().find(|b| *b == name)?
        }
        "member_expression" => {
            let obj = callee.child_by_field_name("object")?;
            let base = obj.utf8_text(source).ok()?;
            TEST_BASES.iter().copied().find(|b| *b == base)?
        }
        _ => return None,
    };
    let args = node.child_by_field_name("arguments")?;
    let first = args.named_child(0)?;
    let title = string_literal_value(first, source)?;
    Some((kind, title))
}

/// Extract the literal text of a string / template-string-without-
/// substitutions argument. Returns `None` for anything dynamic
/// (concatenation, identifier, template with expressions) — we only
/// flag exact static collisions.
fn string_literal_value(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "string" => {
            let mut cursor = node.walk();
            let mut out = String::new();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "string_fragment" {
                    out.push_str(child.utf8_text(source).ok()?);
                }
            }
            Some(out)
        }
        "template_string" => {
            let mut cursor = node.walk();
            let mut out = String::new();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "string_fragment" => out.push_str(child.utf8_text(source).ok()?),
                    // Any substitution makes the title dynamic — bail.
                    "template_substitution" => return None,
                    _ => {}
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Walk the direct children of `scope` (a `program` or `statement_block`),
/// tracking test titles by construct kind. When a duplicate is found at
/// this level, push a diagnostic. Recurse into describe callback bodies
/// to check their own (independent) scopes.
fn check_scope(
    scope: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // (kind, title) set used to detect duplicates within this scope.
    let mut seen: HashSet<(&'static str, String)> = HashSet::new();
    let mut cursor = scope.walk();
    for child in scope.children(&mut cursor) {
        // Test calls live inside expression statements at scope top.
        if child.kind() != "expression_statement" {
            continue;
        }
        let Some(expr) = child.named_child(0) else {
            continue;
        };
        let Some((kind, title)) = classify_test_call(expr, source) else {
            continue;
        };

        let key = (kind, title.clone());
        if !seen.insert(key) {
            let pos = expr.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-identical-title".into(),
                message: format!(
                    "Duplicate {kind} title {title:?} in the same scope — use a unique title."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        // For describe blocks, recurse into the callback body so nested
        // titles are checked in their own scope.
        if kind == "describe"
            && let Some(args) = expr.child_by_field_name("arguments")
        {
            let ac = args.named_child_count();
            if ac > 0
                && let Some(cb) = args.named_child(ac - 1)
                && let Some(body) = cb.child_by_field_name("body")
                && body.kind() == "statement_block"
            {
                check_scope(body, source, ctx, diagnostics);
            }
        }
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Single traversal from the program root — check_scope recurses
    // itself into describe bodies.
    if node.kind() != "program" {
        return;
    }
    check_scope(node, source, ctx, diagnostics);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_describe_titles() {
        let src = "\
describe('auth', () => {});
describe('auth', () => {});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-identical-title");
    }

    #[test]
    fn flags_duplicate_test_titles_in_same_describe() {
        let src = "\
describe('auth', () => {
  test('rejects empty', () => {});
  test('rejects empty', () => {});
});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_duplicate_it_titles() {
        let src = "\
it('works', () => {});
it('works', () => {});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_only_and_skip_variants_as_same_title() {
        let src = "\
describe('x', () => {});
describe.only('x', () => {});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_same_title_in_different_describes() {
        let src = "\
describe('a', () => {
  test('handles empty', () => {});
});
describe('b', () => {
  test('handles empty', () => {});
});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nested_duplicate_vs_outer() {
        let src = "\
describe('outer', () => {
  test('shared', () => {});
  describe('inner', () => {
    test('shared', () => {});
  });
});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_distinct_titles() {
        let src = "\
describe('auth', () => {
  test('a', () => {});
  test('b', () => {});
});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_same_title_on_different_constructs() {
        // describe('x') and test('x') don't collide — different suite/test scopes.
        let src = "\
describe('x', () => {
  test('x', () => {});
});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_dynamic_titles() {
        let src = "\
const name = 'x';
test(`case ${name}`, () => {});
test(`case ${name}`, () => {});";
        assert!(run_on(src).is_empty());
    }
}
