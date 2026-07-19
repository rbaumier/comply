//! rust-float-eq-partial-cmp backend.
//!
//! For each `binary_expression` whose operator is `==` or `!=`, flag if
//! either operand looks like a float:
//! - `float_literal` (e.g. `1.0`, `1e9`)
//! - identifier whose binding type annotation is `f32`/`f64` — comply
//!   only sees the file we're checking, so we walk back from the operand
//!   to the closest enclosing `let_declaration` / `parameter` / `function_item`
//!   and read the type annotation if present.
//!
//! When the type isn't visible we fall back to "operand is a float
//! literal" — that's the unambiguous case clippy's `float_cmp` also
//! catches first.
//!
//! Skips exact zero, lossless integer round-trip casts, and state-change
//! detection (`old = current; if new == old`) — see the guards in
//! `visit_node`.
//!
//! Also defers to an author's explicit suppression of clippy's equivalent lint:
//! an `#[allow(clippy::float_cmp)]` / `#[allow(clippy::float_cmp_const)]` (or the
//! `#![allow(...)]` inner form on a function body) in scope silences the
//! diagnostic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::has_clippy_allow;

const KINDS: &[&str] = &["binary_expression"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(op) = node.child_by_field_name("operator") else {
            return;
        };
        let Ok(op_text) = op.utf8_text(source) else {
            return;
        };
        if op_text != "==" && op_text != "!=" {
            return;
        }
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            return;
        };
        if !operand_is_float(left, source) && !operand_is_float(right, source) {
            return;
        }
        // Comparing against exact zero is legitimate: `0.0` is exactly
        // representable, so "is this exactly zero?" (e.g. `val.fract() == 0.0`
        // to test integer-ness, or an exact-divisibility `rem == 0.0`) is the
        // correct tool, not an epsilon. Clippy's `float_cmp` skips zero too.
        if operand_is_float_zero(left, source) || operand_is_float_zero(right, source) {
            return;
        }
        // Same exact-representability family: `int as f64 == value` is the
        // lossless integer round-trip idiom ("did casting to an int and back
        // lose anything?"). Exact equality is the only correct tool — an
        // epsilon would let a near-integer wrongly pass.
        if operand_is_int_to_float_cast(left, source)
            || operand_is_int_to_float_cast(right, source)
        {
            return;
        }
        // State-change detection: `old = current; … if new == old { … }`. When
        // both operands are plain identifiers and one was just *stored to* via a
        // bare `x = …;` assignment in an enclosing block, this compares a freshly
        // read value against a previously captured one to detect whether the
        // exact value changed. Both come from the same deterministic source, so
        // exact `==`/`!=` is correct — an epsilon would miss real changes. A
        // naive `computed == 0.1` has a literal operand (no store), so it still
        // fires.
        if is_change_detection(left, right, source) {
            return;
        }
        // Honor the author's explicit suppression of clippy's equivalent lint:
        // `rust-float-eq-partial-cmp` is the comply analog of `clippy::float_cmp`,
        // so an `#[allow(clippy::float_cmp)]` / `#[allow(clippy::float_cmp_const)]`
        // (outer on the function, or inner `#![allow(...)]` in its body) in scope
        // declares the comparison intentional.
        if has_clippy_allow(node, source, "float_cmp")
            || has_clippy_allow(node, source, "float_cmp_const")
        {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-float-eq-partial-cmp",
            format!(
                "float `{op_text}` compares bit patterns, not numerical \
                 equality. Use `(a - b).abs() < EPSILON` with a \
                 domain-appropriate epsilon, or `partial_cmp` for ordering."
            ),
            Severity::Error,
        ));
    }
}

fn operand_is_float(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "float_literal" {
        return true;
    }
    // `as f32` / `as f64` casts.
    if node.kind() == "type_cast_expression"
        && let Some(ty) = node.child_by_field_name("type")
        && let Ok(text) = ty.utf8_text(source)
        && (text == "f32" || text == "f64")
    {
        return true;
    }
    // identifier with a `let x: f32 = …` binding visible in this file.
    if node.kind() == "identifier"
        && let Ok(name) = node.utf8_text(source)
        && let Some(ty) = lookup_let_type(node, name, source)
        && (ty == "f32" || ty == "f64")
    {
        return true;
    }
    false
}

/// Is `node` a cast of an integer expression to `f32`/`f64`, e.g.
/// `(value as u32) as f64` or `i as f64` where `i` is an integer local?
/// Such a comparison is a lossless integer round-trip / exact-representability
/// check, not a fuzzy numerical comparison.
fn operand_is_int_to_float_cast(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "type_cast_expression" {
        return false;
    }
    let Some(ty) = node.child_by_field_name("type") else {
        return false;
    };
    let Ok(ty_text) = ty.utf8_text(source) else {
        return false;
    };
    if ty_text != "f32" && ty_text != "f64" {
        return false;
    }
    let Some(inner) = node.child_by_field_name("value") else {
        return false;
    };
    operand_is_integer(inner, source)
}

/// Is `node` an integer expression: a cast to an integer type
/// (`x as i64`), or an identifier bound to an integer-typed local in this file?
fn operand_is_integer(node: tree_sitter::Node, source: &[u8]) -> bool {
    let node = unwrap_parens(node);
    // `<expr> as <int type>`.
    if node.kind() == "type_cast_expression"
        && let Some(ty) = node.child_by_field_name("type")
        && let Ok(text) = ty.utf8_text(source)
        && is_integer_type(text)
    {
        return true;
    }
    // identifier bound to an integer-typed local visible in this file, via
    // either a type annotation (`let i: u32 = …`) or an integer-cast
    // initializer (`let i = value as u32;`).
    if node.kind() == "identifier"
        && let Ok(name) = node.utf8_text(source)
        && lookup_let_int_binding(node, name, source)
    {
        return true;
    }
    false
}

/// Peel `parenthesized_expression` wrappers, e.g. `(n as i64)` -> `n as i64`.
fn unwrap_parens(mut node: tree_sitter::Node) -> tree_sitter::Node {
    while node.kind() == "parenthesized_expression" {
        match node.named_child(0) {
            Some(inner) => node = inner,
            None => break,
        }
    }
    node
}

/// The set of Rust integer primitive type names.
fn is_integer_type(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
    )
}

/// Is `node` a float-zero literal? Covers `0.0`, `0.0f64`, `0.`, a
/// leading-minus `-0.0`, and the suffixed-integer forms `0f32`/`0f64` — a zero
/// written without a decimal point parses as an `integer_literal` (with an
/// `f32`/`f64` suffix), not a `float_literal`, in the tree-sitter grammar. A
/// negative zero appears as a `unary_expression` (`-` applied to the literal),
/// so unwrap it first.
fn operand_is_float_zero(node: tree_sitter::Node, source: &[u8]) -> bool {
    let lit = if node.kind() == "unary_expression" {
        match node.child_by_field_name("operator").and_then(|o| o.utf8_text(source).ok()) {
            Some("-") => match node.named_child(0) {
                Some(inner) => inner,
                None => return false,
            },
            _ => return false,
        }
    } else {
        node
    };
    let Ok(text) = lit.utf8_text(source) else {
        return false;
    };
    // A float zero is either a `float_literal` (`0.0`, `0.0f32`, `0.`) or an
    // `integer_literal` carrying an `f32`/`f64` suffix (`0f32`, `0f64`). Require
    // the suffix for integer literals: it is what marks the token as a float —
    // a bare `0` is an integer and must not be treated as float-zero here.
    let is_typed_float_literal = lit.kind() == "float_literal"
        || (lit.kind() == "integer_literal"
            && (text.ends_with("f32") || text.ends_with("f64")));
    if !is_typed_float_literal {
        return false;
    }
    // Strip an optional `f32`/`f64` type suffix, then check the numeric part is
    // zero (`0`, `0.`, `0.0`, `0.000`, `0e0`, `0f32`).
    let numeric = text.trim_end_matches("f64").trim_end_matches("f32");
    numeric
        .parse::<f64>()
        .is_ok_and(|value| value == 0.0)
}

/// Walk upward from `node`, scanning preceding siblings at each level for a
/// `let_declaration`, and return the first `extract(decl)` that is `Some`.
fn find_let_decl<T>(
    node: tree_sitter::Node,
    extract: impl Fn(tree_sitter::Node) -> Option<T>,
) -> Option<T> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        let mut sibling = cur.prev_named_sibling();
        while let Some(s) = sibling {
            if s.kind() == "let_declaration"
                && let Some(found) = extract(s)
            {
                return Some(found);
            }
            sibling = s.prev_named_sibling();
        }
        cur = parent;
    }
    None
}

/// Walk upward from `node` looking for a `let_declaration` whose pattern
/// names `ident`. If found, return its type annotation text.
fn lookup_let_type(node: tree_sitter::Node, ident: &str, source: &[u8]) -> Option<String> {
    find_let_decl(node, |decl| let_decl_type_for(decl, ident, source))
}

/// Is `ident` bound to an integer-typed local visible above `node`? Recognises
/// both a type annotation (`let i: u32 = …`) and an integer-cast initializer
/// (`let i = value as u32;`).
fn lookup_let_int_binding(node: tree_sitter::Node, ident: &str, source: &[u8]) -> bool {
    find_let_decl(node, |decl| {
        let pat = decl.child_by_field_name("pattern")?;
        if pat.utf8_text(source).ok()? != ident {
            return None;
        }
        let is_int = decl
            .child_by_field_name("type")
            .and_then(|ty| ty.utf8_text(source).ok())
            .is_some_and(is_integer_type)
            || decl
                .child_by_field_name("value")
                .is_some_and(|init| init_is_int_cast(init, source));
        is_int.then_some(())
    })
    .is_some()
}

/// Is the initializer expression an integer cast (`value as u32`)?
fn init_is_int_cast(init: tree_sitter::Node, source: &[u8]) -> bool {
    init.kind() == "type_cast_expression"
        && init
            .child_by_field_name("type")
            .and_then(|ty| ty.utf8_text(source).ok())
            .is_some_and(is_integer_type)
}

/// Is this comparison a state-change detector — `old = current; if new == old`?
///
/// Requires both operands to be plain identifiers (no float literal on either
/// side; a literal means a fixed-threshold compare, not change detection) and
/// at least one operand to be the assignment *target* of a bare
/// `x = …;` statement in an enclosing block. Capturing a value into a variable
/// and later comparing a fresh reading against it is exact-equality by design.
fn is_change_detection(left: tree_sitter::Node, right: tree_sitter::Node, source: &[u8]) -> bool {
    if left.kind() != "identifier" || right.kind() != "identifier" {
        return false;
    }
    let (Ok(left_name), Ok(right_name)) = (left.utf8_text(source), right.utf8_text(source)) else {
        return false;
    };
    // Store *presence* (not store-then-compare ordering) is intentional: a
    // captured-then-reassigned float local compared to another float local is
    // the change-detection shape; demanding exact ordering would add fragility
    // for no real precision gain.
    ident_stored_in_enclosing_block(left, left_name, source)
        || ident_stored_in_enclosing_block(right, right_name, source)
}

/// Walk upward from `node`, scanning every statement in each enclosing block for
/// an `assignment_expression` whose left-hand side is exactly `ident`.
fn ident_stored_in_enclosing_block(node: tree_sitter::Node, ident: &str, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "block" {
            let mut walker = parent.walk();
            for child in parent.named_children(&mut walker) {
                if assignment_lhs_is(child, ident, source) {
                    return true;
                }
            }
        }
        cur = parent;
    }
    false
}

/// Is `node` an `expression_statement` wrapping `ident = …` (or `ident = …`
/// directly)?
fn assignment_lhs_is(node: tree_sitter::Node, ident: &str, source: &[u8]) -> bool {
    let expr = if node.kind() == "expression_statement" {
        match node.named_child(0) {
            Some(inner) => inner,
            None => return false,
        }
    } else {
        node
    };
    expr.kind() == "assignment_expression"
        && expr
            .child_by_field_name("left")
            .and_then(|lhs| lhs.utf8_text(source).ok())
            == Some(ident)
}

fn let_decl_type_for(decl: tree_sitter::Node, ident: &str, source: &[u8]) -> Option<String> {
    let pat = decl.child_by_field_name("pattern")?;
    let pat_text = pat.utf8_text(source).ok()?;
    if pat_text != ident {
        return None;
    }
    let ty = decl.child_by_field_name("type")?;
    Some(ty.utf8_text(source).ok()?.to_string())
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
    fn flags_float_literal_eq() {
        let src = "fn f(x: f64) -> bool { x == 1.0 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_float_literal_neq() {
        let src = "fn f(x: f64) -> bool { x != 1.0 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_let_typed_float_eq() {
        let src = "fn f() -> bool { let x: f32 = 1.0; x == 2.0 }";
        // 1.0 makes left float-literal-like once typed, but the right side
        // alone (1.0 / 2.0) is already a float_literal. Either way we only
        // report once per binary_expression.
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_int_eq() {
        let src = "fn f(x: u32) -> bool { x == 1 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_partial_cmp() {
        let src = "fn f(a: f64, b: f64) -> Option<std::cmp::Ordering> { a.partial_cmp(&b) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fract_eq_zero() {
        // tantivy columnar/src/value.rs: `fract == 0.0` (let-bound fract()).
        let src = "fn f(val: f64) -> bool { let fract = val.fract(); fract == 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_remainder_eq_zero() {
        let src = "fn f(right_f: f64, right_as_i: i64) -> bool { \
                   let rem = right_f - (right_as_i as f64); rem == 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_direct_fract_call_eq_zero() {
        let src = "fn f(x: f64) -> bool { x.fract() == 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_zero_neq() {
        let src = "fn f(x: f64) -> bool { x != 0.0 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_zero_variants() {
        for src in [
            "fn f(x: f64) -> bool { x == 0.0f64 }",
            "fn f(x: f64) -> bool { x == 0f64 }",
            "fn f(x: f64) -> bool { x == 0. }",
            "fn f(x: f64) -> bool { x == -0.0 }",
            "fn f(x: f64) -> bool { 0.0 == x }",
        ] {
            assert!(run_on(src).is_empty(), "should not flag: {src}");
        }
    }

    #[test]
    fn allows_suffixed_int_zero_against_float_let_binding() {
        // image-rs/image imageops/sample.rs: `let mut sum_norm: f32 = 0f32; if
        // sum_norm != 0f32 { .. }`. `0f32` is a suffixed integer literal (no
        // decimal point) but exactly-representable zero, so the guard is exact
        // and correct — same family as the already-exempt `!= 0.0f32`.
        for src in [
            "fn f() { let mut sum_norm: f32 = 0f32; if sum_norm != 0f32 { let _s = sum_norm; } }",
            "fn f() -> bool { let x: f64 = compute(); x == 0f64 }",
        ] {
            assert!(run_on(src).is_empty(), "should not flag: {src}");
        }
    }

    #[test]
    fn flags_nonzero_suffixed_int_literal() {
        // Negative space: a non-zero suffixed integer float literal still fires —
        // the exemption is scoped to zero only.
        assert_eq!(
            run_on("fn f() -> bool { let x: f32 = compute(); x != 1f32 }").len(),
            1
        );
    }

    #[test]
    fn flags_nonliteral_operand_against_float_let_binding() {
        // Negative space: a non-literal right operand (identifier) is unchanged
        // by the zero-exemption path and still fires.
        assert_eq!(
            run_on("fn f(y: f32) -> bool { let x: f32 = compute(); x != y }").len(),
            1
        );
    }

    #[test]
    fn allows_int_roundtrip_cast_initializer() {
        // oxc constant_evaluation/call_expr.rs: `let i = value as u32; i as f64
        // != value` — the lossless integer round-trip idiom (no annotation).
        let src = "fn f(value: f64) -> Option<u32> { \
                   let i = value as u32; if i as f64 != value { return None; } Some(i) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_inline_nested_int_cast() {
        let src = "fn f(n: i32, value: f64) -> bool { (n as i64) as f64 == value }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_annotated_int_local_roundtrip() {
        let src = "fn f(value: f64) -> bool { let i: u32 = something(); i as f64 != value }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_state_change_detection() {
        // winit-win32/src/event_loop.rs: `old = current; if new == old { return }`
        // — compare a freshly read OS value against the previously stored one.
        let src = "fn f() { \
                   let old_scale_factor: f64; \
                   { old_scale_factor = window_state.scale_factor; \
                   window_state.scale_factor = new_scale_factor; \
                   if new_scale_factor == old_scale_factor { return; } } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_change_detection_neq() {
        let src = "fn f(new: f64) -> bool { let prev: f64; prev = read(); prev != new }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_naive_literal_threshold_still() {
        // Negative space for the change-detection exemption: a fixed-threshold
        // compare has a literal operand and no store, so it still fires even
        // when the other operand was assigned nearby.
        let src = "fn f() -> bool { let ratio: f64; ratio = compute(); ratio == 0.1 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_nonzero_float_literal_eq() {
        // Negative space: a genuine epsilon-needing comparison still fires.
        assert_eq!(run_on("fn f(x: f64) -> bool { x == 1.5 }").len(), 1);
    }

    #[test]
    fn flags_nonzero_sum_eq() {
        assert_eq!(run_on("fn f(a: f64, b: f64) -> bool { (a + b) == 0.3 }").len(), 1);
    }

    #[test]
    fn allows_inner_clippy_float_cmp_allow() {
        // sharkdp/pastel src/types.rs: the author placed an inner
        // `#![allow(clippy::float_cmp)]` declaring the comparison intentional.
        let src = "fn value(unclipped: f64) -> f64 { \
                   #![allow(clippy::float_cmp)] \
                   if unclipped == 360.0 { unclipped } else { wrap(unclipped) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_outer_clippy_float_cmp_allow() {
        let src = "#[allow(clippy::float_cmp)] fn f(x: f64) -> bool { x == 1.5 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_clippy_float_cmp_const_allow() {
        let src = "#[allow(clippy::float_cmp_const)] fn f(x: f64) -> bool { x == 1.5 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_without_any_allow() {
        // Negative control: no suppression attribute → still fires.
        assert_eq!(run_on("fn f(x: f64) -> bool { x == 1.5 }").len(), 1);
    }

    #[test]
    fn flags_with_unrelated_allow() {
        // Negative control: an unrelated allow does not suppress.
        for src in [
            "#[allow(dead_code)] fn f(x: f64) -> bool { x == 1.5 }",
            "#[allow(clippy::approx_constant)] fn f(x: f64) -> bool { x == 1.5 }",
            "fn f(x: f64) -> bool { #![allow(dead_code)] x == 1.5 }",
        ] {
            assert_eq!(run_on(src).len(), 1, "should still flag: {src}");
        }
    }
}
