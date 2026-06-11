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
//! Matches whose enum-like arms all reference a known stdlib closed or
//! non_exhaustive enum — `Result` (`Ok`/`Err`), `Option` (`Some`/`None`),
//! or `std::io::ErrorKind` — are exempt: the wildcard there is idiomatic
//! or compiler-mandated, and all arms of a `match` share one type.
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

        // Walk the match_arm children, collecting wildcard arms and the
        // patterns of arms that look enum-like.
        let mut wildcard_arms: Vec<tree_sitter::Node> = Vec::new();
        let mut enum_like_arms: Vec<tree_sitter::Node> = Vec::new();
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
                enum_like_arms.push(pattern);
            }
        }

        if enum_like_arms.is_empty() {
            return;
        }
        // All arms of a `match` necessarily cover the same type, so when
        // every enum-like arm references a known stdlib closed or
        // non_exhaustive enum, the scrutinee is that stdlib type and the
        // wildcard is idiomatic (Result/Option) or compiler-mandated
        // (ErrorKind) — never a silent catch-all for a project enum.
        if enum_like_arms
            .iter()
            .all(|p| references_stdlib_closed_enum(*p, source_bytes))
        {
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

/// True if `pattern` references a variant of a known stdlib closed or
/// non_exhaustive enum: `Result` (`Ok`/`Err`), `Option` (`Some`/`None`),
/// or `std::io::ErrorKind`. Matching is purely syntactic: the final path
/// segment of the variant head must be exactly one of the Result/Option
/// constructors, or the head must contain `ErrorKind::`.
fn references_stdlib_closed_enum(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    // Unwrap the `match_pattern` wrapper, mirroring `pattern_is_enum_like`.
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        if let Some(inner) = pattern.named_children(&mut cursor).next() {
            return references_stdlib_closed_enum(inner, source);
        }
        return false;
    }
    // Or-pattern: every disjunct must reference a stdlib enum.
    if pattern.kind() == "or_pattern" {
        let mut cursor = pattern.walk();
        return pattern
            .named_children(&mut cursor)
            .all(|child| references_stdlib_closed_enum(child, source));
    }

    let Ok(text) = pattern.utf8_text(source) else {
        return false;
    };
    let text = text.trim();
    // Strip tuple-struct fields: `Err(e)` → `Err`, `Some(v)` → `Some`.
    let head = text.split('(').next().unwrap_or(text).trim();
    // Final path segment: `Result::Ok` → `Ok`, `Option::Some` → `Some`.
    let last_seg = head.rsplit("::").next().unwrap_or(head).trim();
    if matches!(last_seg, "Ok" | "Err" | "Some" | "None") {
        return true;
    }
    // `std::io::ErrorKind` is #[non_exhaustive]: a `_` arm is mandatory.
    head.contains("ErrorKind::")
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
    fn allows_wildcard_with_option_variants() {
        let src = "fn f(x: Option<i32>) -> i32 { match x { Some(v) => v, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_result_variants() {
        let src = "fn f(r: Result<i32, E>) -> i32 { match r { Err(e) => 1, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_errorkind() {
        let src = "fn f(e: std::io::Error) -> i32 { \
                   match e.kind() { ErrorKind::PermissionDenied => 1, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_qualified_result() {
        let src = "fn f(r: Result<i32, E>) -> i32 { match r { Result::Ok(v) => v, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_project_variant_resembling_ok() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::OkResponse => 1, _ => 0 } }";
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
