//! rust-explicit-enum-match-arms backend.
//!
//! Walks every `match_expression`, looks at its arms, and flags a lone
//! `_` arm when at least one other arm has a pattern that "looks like"
//! an enum variant. See the module-level docblock in `mod.rs` for the
//! heuristic rationale.
//!
//! Pattern classification is purely syntactic:
//!
//! - "wildcard": node kind `wildcard_pattern`, or a pattern whose full
//!   text is exactly `_`.
//! - "enum-like": node kind is one of `scoped_identifier`,
//!   `tuple_struct_pattern`, `struct_pattern`, or the pattern text
//!   either contains `::` or starts with an ASCII uppercase letter.
//!   Or-patterns (`Foo::A | Foo::B`) are unwrapped and any disjunct
//!   that qualifies makes the whole arm enum-like.
//!
//! We do not descend into nested `match`es here — the walker visits
//! every `match_expression` independently, so each match is classified
//! on its own arms.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["match_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if is_in_test_context(node, source_bytes) {
            return;
        }
        let Some(match_block) = node.child_by_field_name("body") else {
            return;
        };

        // Walk the match_arm children, collecting wildcard arms and
        // noting whether any arm looks enum-like.
        let mut wildcard_arms: Vec<tree_sitter::Node> = Vec::new();
        let mut has_enum_like_arm = false;
        let mut cursor = match_block.walk();
        for child in match_block.named_children(&mut cursor) {
            if child.kind() != "match_arm" {
                continue;
            }
            let Some(pattern) = child.child_by_field_name("pattern") else {
                continue;
            };
            if pattern_is_wildcard(pattern, source_bytes) {
                wildcard_arms.push(child);
            } else if pattern_is_enum_like(pattern, source_bytes) {
                has_enum_like_arm = true;
            }
        }

        if !has_enum_like_arm {
            return;
        }
        // Emit on each wildcard arm found (usually just one).
        for arm in wildcard_arms {
            let pos = arm.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-explicit-enum-match-arms".into(),
                message: "Wildcard `_` arm in a `match` that appears to cover an enum. \
                          List each variant explicitly so adding a new variant produces \
                          a compile error at this `match`, forcing a decision instead of \
                          silently falling through."
                    .into(),
                severity: Severity::Warning,
                span: Some((arm.start_byte(), arm.end_byte() - arm.start_byte())),
            });
        }
    }
}

/// True if `pattern` is a bare wildcard `_`.
fn pattern_is_wildcard(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    if pattern.kind() == "wildcard_pattern" {
        return true;
    }
    // Fallback: some grammar versions may surface `_` as an identifier
    // or similar — trust the textual form only when it's exactly `_`.
    matches!(pattern.utf8_text(source), Ok("_"))
}

/// True if `pattern` looks like it matches an enum variant. See module
/// docblock for the heuristic.
fn pattern_is_enum_like(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    // tree-sitter-rust wraps match arm patterns in a `match_pattern` node
    // (to accommodate guard clauses like `pat if cond`). Unwrap to the
    // inner pattern before classifying.
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        if let Some(inner) = pattern.named_children(&mut cursor).next() {
            return pattern_is_enum_like(inner, source);
        }
        return false;
    }
    // Tuple patterns are product types: wildcard is always idiomatic
    // (covering N×M combinations of sub-arms is impractical).
    if pattern.kind() == "tuple_pattern" {
        return false;
    }
    // Or-pattern: recurse into each disjunct.
    if pattern.kind() == "or_pattern" {
        let mut cursor = pattern.walk();
        for child in pattern.named_children(&mut cursor) {
            if pattern_is_enum_like(child, source) {
                return true;
            }
        }
        return false;
    }

    match pattern.kind() {
        "scoped_identifier" | "tuple_struct_pattern" | "struct_pattern" => return true,
        _ => {}
    }

    let Ok(text) = pattern.utf8_text(source) else {
        return false;
    };
    let text = text.trim();
    if text.is_empty() || text == "_" {
        return false;
    }
    if text.contains("::") {
        return true;
    }
    // Leading identifier starts with ASCII uppercase → looks like a
    // variant or tuple struct (e.g. `Some(x)`, `None`, `Direction`).
    let first_ident_char = text
        .chars()
        .find(|c| c.is_ascii_alphanumeric() || *c == '_');
    matches!(first_ident_char, Some(c) if c.is_ascii_uppercase())
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_wildcard_with_enum_variants() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, Foo::B => 2, _ => 3 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_with_option_variants() {
        let src = "fn f(x: Option<i32>) -> i32 { match x { Some(v) => v, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_with_path_variants() {
        let src = "fn f(x: Direction) -> i32 { match x { Direction::North => 1, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_all_variants_explicit() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, Foo::B => 2, Foo::C => 3 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_integer_match() {
        let src = "fn f(x: i32) -> i32 { match x { 1 => 10, 2 => 20, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_wildcard_arm() {
        let src = "fn f(x: i32) -> i32 { match x { _ => 42 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_or_patterns() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A | Foo::B => 1, Foo::C => 2 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let src = "#[test]\nfn t() { let x = Foo::A; let _ = match x { Foo::A => 1, _ => 2 }; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_tuple_of_options() {
        let src = "fn f(x: (Option<i32>, Option<i32>)) -> i32 { \
                   match x { (Some(a), Some(b)) => a + b, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_tuple_of_results() {
        let src = "fn f(x: (Result<i32, E>, Result<i32, E>)) -> i32 { \
                   match x { (Ok(a), Ok(b)) => a + b, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }
}
