//! justify-inaction Rust backend.
//!
//! Flags empty control-flow blocks that have no comment inside
//! explaining why. Targets:
//!
//! - `if_expression.consequence` — empty `if cond { }`.
//! - `else_clause`'s `block` child — empty `else { }`.
//! - `match_arm.value` when the value is an empty `block` AND the
//!   pattern is an error-ignoring `Err(…)` — silently swallowing an
//!   error deserves a justification. Wildcard arms (`_ => {}`) and
//!   named variant no-ops (`Progress::None => {}`) are exempt: the
//!   explicit catch-all / variant name documents the intent, and the
//!   wildcard arm is required for match exhaustiveness anyway.
//!   An `Err(…)` arm is also exempt when every variable it binds is
//!   underscore-prefixed (e.g. `Err(_frame) => {}`): the `_` prefix is
//!   Rust's signal that the value is intentionally discarded, so the
//!   empty body is already documented by the binding name.
//! - `for_expression.body` / `while_expression.body` /
//!   `loop_expression.body` — empty loop body.
//!
//! A block is considered justified and NOT flagged if it contains at
//! least one comment child (`line_comment` / `block_comment`). Any
//! other named child also makes the block non-empty by definition.
//!
//! Function bodies, closure bodies, and empty `{}` used as unit
//! expressions in other positions are out of scope — they are common
//! in stubs, marker impls, and no-op callbacks, and flagging them
//! would be pure noise.

use crate::diagnostic::{Diagnostic, Severity};

fn block_is_empty(node: tree_sitter::Node) -> bool {
    node.kind() == "block" && node.named_child_count() == 0
}

fn match_arm_needs_justification(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(pattern) = arm.child_by_field_name("pattern") else {
        return true;
    };
    pattern_needs_justification(pattern, source)
}

fn pattern_needs_justification(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "_" | "wildcard_pattern" => false,
        "match_pattern" | "or_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if pattern_needs_justification(child, source) {
                    return true;
                }
            }
            false
        }
        "tuple_struct_pattern" => {
            let Ok(text) = node.utf8_text(source) else {
                return false;
            };
            if !(text.starts_with("Err(") || text.contains("::Err(")) {
                return false;
            }
            !all_bindings_underscore_prefixed(node, source)
        }
        _ => false,
    }
}

/// True when the pattern introduces at least one variable binding and every
/// such binding is underscore-prefixed (Rust's signal for intentional
/// discard, e.g. `Err(_frame)`). Bindings are the `identifier` children of
/// the pattern other than the constructor path (`type` field of a
/// `tuple_struct_pattern`). A pattern with no bindings — e.g. `Err(_)` whose
/// `_` is a bare wildcard, not a binding — returns false.
fn all_bindings_underscore_prefixed(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut binding_count = 0usize;
    let mut cursor = node.walk();
    for (i, child) in node.children(&mut cursor).enumerate() {
        if child.kind() != "identifier" {
            continue;
        }
        if node.field_name_for_child(i as u32) == Some("type") {
            continue;
        }
        binding_count += 1;
        if !child.utf8_text(source).is_ok_and(|name| name.starts_with('_')) {
            return false;
        }
    }
    binding_count > 0
}

fn loop_name(kind: &str) -> &'static str {
    match kind {
        "for_expression" => "for",
        "while_expression" => "while",
        "loop_expression" => "loop",
        _ => "loop",
    }
}

fn flag_empty(
    container: tree_sitter::Node,
    body: tree_sitter::Node,
    what: &str,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !block_is_empty(body) {
        return;
    }
    let pos = container.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "justify-inaction".into(),
        message: format!(
            "Empty `{what}` block \u{2014} add a comment inside explaining why the inaction is intentional."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

crate::ast_check! { on ["if_expression", "else_clause", "match_arm", "for_expression", "while_expression", "loop_expression"] => |node, _source, ctx, diagnostics|
match node.kind() {
        "if_expression" => {
            if let Some(cons) = node.child_by_field_name("consequence") {
                flag_empty(node, cons, "if", ctx, diagnostics);
            }
        }
        "else_clause" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "block" {
                    flag_empty(node, child, "else", ctx, diagnostics);
                    break;
                }
            }
        }
        "match_arm" => {
            if let Some(value) = node.child_by_field_name("value")
                && block_is_empty(value) && match_arm_needs_justification(node, _source) {
                    flag_empty(node, value, "match arm", ctx, diagnostics);
                }
        }
        "for_expression" | "while_expression" | "loop_expression" => {
            if let Some(body) = node.child_by_field_name("body") {
                flag_empty(node, body, loop_name(node.kind()), ctx, diagnostics);
            }
        }
        _ => {}
    }
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

    // -- if / else --

    #[test]
    fn flags_empty_if() {
        assert_eq!(run_on("fn f(x: bool) { if x {} }").len(), 1);
    }

    #[test]
    fn flags_empty_else() {
        let src = "fn f(x: bool) { if x { go(); } else {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_if_with_comment_inside() {
        let src = "fn f(x: bool) { if x { /* handled upstream */ } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_else_with_line_comment_inside() {
        let src = "fn f(x: bool) {\n    if x {\n        go();\n    } else {\n        // intentional no-op\n    }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_empty_else() {
        let src = "fn f(x: bool) { if x { a(); } else { b(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_else_if_chain() {
        let src = "fn f(x: i32) { if x == 1 { a(); } else if x == 2 { b(); } }";
        assert!(run_on(src).is_empty());
    }

    // -- match arms --

    #[test]
    fn allows_empty_named_variant_arm() {
        let src = "fn f(x: Option<u8>) { match x { Some(v) => go(v), None => {} } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_scoped_variant_arm() {
        let src = "fn f(x: u8) { match x { Progress::Active(v) => go(v), Progress::None => {} } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_literal_arm() {
        let src = "fn f(x: u8) { match x { 0 => {}, 1 => go() } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_empty_err_arm() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(_) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_empty_wildcard_arm() {
        let src = "fn f(x: u8) { match x { 0 => go(), _ => {} } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_err_arm_with_underscore_binding_issue_1444() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(_frame) => {} } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_or_arm_all_underscore_bindings() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(_a) | Err(_b) => {} } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_err_arm_with_named_binding() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(e) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_err_arm_with_named_frame_binding() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(frame) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_scoped_err_arm_with_named_binding() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), my::Err(e) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_empty_wildcard_arm_issue_1002() {
        let src = "fn f(v: E) { match v { E::Specific(fld) => { go(fld); } _ => {} } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_unit_wildcard_arm() {
        let src = "fn f(x: u8) { match x { 0 => go(), _ => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_match_arm_with_comment_inside() {
        let src = r#"
fn f(x: Option<u8>) {
    match x {
        Some(v) => go(v),
        None => {
            // already handled upstream
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_empty_match_arm() {
        let src = "fn f(x: u8) { match x { 0 => {}, _ => go() } }";
        assert!(run_on(src).is_empty());
    }

    // -- loops --

    #[test]
    fn flags_empty_while() {
        assert_eq!(run_on("fn f() { while poll() {} }").len(), 1);
    }

    #[test]
    fn flags_empty_for() {
        assert_eq!(run_on("fn f(xs: &[u8]) { for _ in xs {} }").len(), 1);
    }

    #[test]
    fn flags_empty_loop() {
        assert_eq!(run_on("fn f() { loop {} }").len(), 1);
    }

    #[test]
    fn allows_while_with_comment() {
        let src = "fn f() { while poll() { /* busy-wait for the device */ } }";
        assert!(run_on(src).is_empty());
    }

    // -- scope exclusions --

    #[test]
    fn does_not_flag_empty_fn_body() {
        assert!(run_on("fn stub() {}").is_empty());
    }

    #[test]
    fn does_not_flag_empty_closure_body() {
        let src = "fn f() { let cb = || {}; cb(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_unit_match_arm() {
        let src = "fn f(x: Option<u8>) { match x { Some(v) => go(v), None => () } }";
        assert!(run_on(src).is_empty());
    }
}
