//! no-invariant-returns Rust backend.
//!
//! Walks `function_item` nodes, collects the `return_expression` nodes
//! belonging directly to the function body plus the function block's tail
//! expression (Rust's implicit return), and flags the function only when every
//! return site is provably the same literal. A return site whose value is not a
//! literal (a computed expression, or a control-flow tail such as `if`/`match`)
//! makes invariance unprovable, so the function is left unflagged.
//!
//! Nested `function_item` and `closure_expression` subtrees are skipped so
//! an inner closure's `return` is not attributed to the outer function.
//!
//! A function whose body contains a `?` (`try_expression`) is left unflagged:
//! the `?` short-circuits on the function's own `Option`/`Result` carrier, so
//! the return type is load-bearing control flow rather than a value that could
//! be a constant.
//!
//! A method of a *trait* implementation (`impl Trait for Type { … }`) is also
//! left unflagged: the trait contract dictates the return type and its
//! invariant value (e.g. a callback whose `bool` return is a continue/abort
//! protocol sentinel), so an invariant return there is contract-mandated rather
//! than a value that should be a constant.

use crate::diagnostic::{Diagnostic, Severity};

/// Recursively scan `node`'s subtree for `return_expression` nodes,
/// stopping at nested function/closure boundaries so inner returns
/// are attributed to the inner function only.
fn collect_returns<'t>(node: tree_sitter::Node<'t>, out: &mut Vec<tree_sitter::Node<'t>>) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        match child.kind() {
            "function_item" | "closure_expression" => {
                // Skip — its returns belong to that inner function.
            }
            "return_expression" => {
                out.push(child);
            }
            _ => collect_returns(child, out),
        }
    }
}

/// Whether `node`'s subtree contains a `try_expression` (`?`), stopping at
/// nested function/closure boundaries so a `?` inside an inner closure is
/// attributed to that closure only — mirroring `collect_returns`.
fn contains_try(node: tree_sitter::Node) -> bool {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        match child.kind() {
            "function_item" | "closure_expression" => {
                // Skip — its `?` belongs to that inner function.
            }
            "try_expression" => return true,
            _ if contains_try(child) => return true,
            _ => {}
        }
    }
    false
}

/// Whether `node` (a `function_item`) is a method of a *trait* implementation
/// (`impl Trait for Type { … }`). A trait method is a direct child of the impl
/// block's `declaration_list`, whose parent `impl_item` carries a `trait` field
/// naming the implemented trait; an inherent impl (`impl Type`) has no `trait`
/// field. Checking this exact two-level parent chain is inherently bounded: a
/// local function nested in a method body is a child of a `block` (not a
/// `declaration_list`) and a free function is a child of `source_file` or a
/// module's `declaration_list`, so neither is matched.
fn is_trait_impl_method(node: tree_sitter::Node) -> bool {
    let Some(list) = node.parent() else { return false };
    if list.kind() != "declaration_list" {
        return false;
    }
    let Some(impl_item) = list.parent() else { return false };
    impl_item.kind() == "impl_item" && impl_item.child_by_field_name("trait").is_some()
}

/// Extract a normalized literal text from a `return_expression` value, or
/// from a tail expression. Returns `None` for non-literals.
fn literal_text<'a>(value: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let kind = value.kind();
    let text = value.utf8_text(source).ok()?.trim();
    match kind {
        "integer_literal" | "float_literal" | "string_literal" | "char_literal"
        | "boolean_literal" | "raw_string_literal" => Some(text),
        // `None` shows up as a regular identifier in expression position.
        "identifier" if text == "None" => Some(text),
        _ => None,
    }
}

/// Pull the value of a `return_expression`, if any (bare `return` has no
/// child). Returns `None` for bare returns and non-literal values.
fn return_value_literal<'a>(ret: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let value = ret.named_child(0)?;
    literal_text(value, source)
}

/// Return the block's trailing expression (Rust's implicit return), if any.
///
/// Bare-value tails (literals, identifiers like `None`) appear directly as the
/// last named child. Block-like tails (`if`, `match`, `loop`, …) are wrapped in
/// an `expression_statement` that — unlike a real statement — carries no
/// trailing `;`; that wrapper is unwrapped to expose the actual tail
/// expression. Statements (`let_declaration`, or an `expression_statement`
/// terminated by `;`) are not tails and yield `None`.
fn block_tail_expression<'t>(block: tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    if block.kind() != "block" {
        return None;
    }
    let last = block.named_child(block.named_child_count().checked_sub(1)?)?;
    match last.kind() {
        "let_declaration" => None,
        "expression_statement" => {
            // A trailing `;` makes this a statement, not an implicit return.
            let has_semicolon = last.child(last.child_count().checked_sub(1)?)?.kind() == ";";
            if has_semicolon {
                None
            } else {
                last.named_child(0)
            }
        }
        _ => Some(last),
    }
}

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    // An `extern`-ABI function (`extern "C" fn`, incl. `unsafe extern "C"`) is an
    // FFI entry point handed to foreign code as a callable, not a value: its
    // return type and any invariant status code are fixed by the external ABI
    // contract and cannot be replaced by a constant. Skip it.
    if crate::rules::rust_helpers::fn_is_extern(node, source) {
        return;
    }

    // A method inside a trait impl (`impl Trait for Type { … }`) has its return
    // type and invariant value dictated by the trait contract — e.g. a cURL
    // `progress` callback whose `bool` return is a continue/abort protocol
    // sentinel — so an invariant return is contract-mandated, not a value that
    // should become a constant. Inherent-impl methods and free functions stay
    // subject to the check.
    if is_trait_impl_method(node) {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };

    // A `?` short-circuiting on the function's own `Option`/`Result` carrier
    // makes the return type load-bearing control flow: each `?` is an implicit
    // early return the literal-return scan never sees, so the terminal literal
    // is the exhausted-all-early-exits fallthrough, not an invariant value that
    // should be a constant (a `?`-carrying function cannot become a constant).
    if contains_try(body) {
        return;
    }

    let mut returns: Vec<tree_sitter::Node> = Vec::new();
    collect_returns(body, &mut returns);

    let mut literals: Vec<&str> = Vec::new();
    for ret in &returns {
        let Some(lit) = return_value_literal(*ret, source) else {
            return; // Non-literal return — can't prove invariance.
        };
        literals.push(lit);
    }

    if let Some(tail) = block_tail_expression(body) {
        let Some(lit) = literal_text(tail, source) else {
            // Tail is a non-literal expression — bail out unless there are
            // no return statements at all (in which case we have nothing).
            return;
        };
        literals.push(lit);
    }

    if literals.len() < 2 {
        return;
    }

    let first = literals[0];
    if !literals.iter().all(|l| *l == first) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-invariant-returns".into(),
        message: "Function always returns the same literal value \u{2014} consider using a constant instead.".into(),
        severity: Severity::Error,
        span: None,
    });
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
    fn flags_invariant_true() {
        let src = r#"
fn is_enabled(x: i32) -> bool {
    if x > 0 {
        return true;
    }
    return true;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_returns() {
        let src = r#"
fn is_positive(n: i32) -> bool {
    if n > 0 {
        return true;
    }
    return false;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_return() {
        let src = r#"
fn get_value() -> i32 {
    return 42;
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Issue #1466 — guard `return None` early-exits plus a non-literal
    // `Some(computed)` happy path in an `if/else` tail must not be flagged.
    #[test]
    fn allows_guard_none_with_some_if_else_tail() {
        let src = r#"
fn literal(&self) -> Option<String> {
    if self.opts.case_insensitive {
        return None;
    }
    let mut lit = String::new();
    for t in &*self.tokens {
        let Token::Literal(c) = *t else { return None };
        lit.push(c);
    }
    if lit.is_empty() { None } else { Some(lit) }
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Issue #1466 — guard `return None` plus a `Some(computed)` happy path in
    // a `match` tail must not be flagged.
    #[test]
    fn allows_guard_none_with_some_match_tail() {
        let src = r#"
fn open(&self, file: &File) -> Option<Mmap> {
    if !self.is_enabled() {
        return None;
    }
    if cfg!(target_os = "macos") {
        return None;
    }
    match unsafe { Mmap::map(file) } {
        Ok(mmap) => Some(mmap),
        Err(_) => None,
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Negative space: a genuinely invariant function whose explicit guard
    // returns and bare implicit tail all yield the same literal must still fire.
    #[test]
    fn flags_invariant_none_across_returns_and_tail() {
        let src = r#"
fn always_none(x: i32) -> Option<i32> {
    if x > 0 {
        return None;
    }
    do_side_effect();
    None
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #7348 — an `unsafe extern "C"` FFI callback (denoland/deno's nghttp2
    // handlers) returns `0` on every path because `0` is the ABI success status,
    // not dead logic. The signature and return value are fixed by the external
    // ABI contract and cannot become a constant, so it must not be flagged.
    #[test]
    fn allows_unsafe_extern_c_ffi_callback() {
        let src = r#"
unsafe extern "C" fn on_stream_close_callback(id: i32, data: *mut c_void) -> i32 {
    let session = unsafe { Session::from_user_data(data) };
    if session.find_stream_obj(id).is_none() {
        return 0;
    }
    session.close_stream(id);
    0
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Issue #7348 — the `extern` guard keys on the ABI modifier, not on `unsafe`,
    // so a safe `extern "C"` function with invariant returns is exempt too.
    #[test]
    fn allows_extern_c_invariant_returns() {
        let src = r#"
extern "C" fn cb(x: i32) -> i32 {
    if x > 0 {
        return 0;
    }
    0
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Issue #7348 — the abi string is optional (`extern fn` defaults to "C"); the
    // guard matches on the `extern` keyword, so this is exempt as well.
    #[test]
    fn allows_extern_without_abi_string() {
        let src = r#"
extern fn cb(x: i32) -> i32 {
    if x > 0 {
        return 0;
    }
    0
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Negative space for #7348: a regular (non-`extern`) function whose return
    // sites all yield the same literal is a genuine invariant return and must
    // still fire — the `extern` guard must not over-suppress ordinary functions.
    #[test]
    fn flags_regular_invariant_returns() {
        let src = r#"
fn regular(x: i32) -> i32 {
    if x > 0 {
        return 0;
    }
    do_stuff();
    0
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #7370 — a side-effecting event handler returning `Option<()>` whose
    // `return None` and tail `None` are identical but whose body uses `?` to
    // short-circuit on its own `Option` carrier. The `?` makes the return type
    // load-bearing control flow, so it must not be flagged.
    #[test]
    fn allows_try_driven_side_effecting_option_fn() {
        let src = r#"
fn local_worktree_entry_changed(this: &mut T) -> Option<()> {
    let id = this.get(&k)?;
    if cond {
        this.remove(&k);
        return None;
    }
    let events = this.update()?;
    for event in events {
        emit(event);
    }
    None
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Negative space for #7370: the `?` bail keys on a `?` at the function's own
    // level. A `?` living only inside a nested closure does not exempt the outer
    // function, whose invariant `None` returns are genuine and must still fire.
    #[test]
    fn flags_invariant_returns_when_try_only_in_nested_closure() {
        let src = r#"
fn outer(x: i32) -> Option<()> {
    let c = || -> Option<()> { probe()?; None };
    if x > 0 {
        return None;
    }
    c();
    side_effect();
    None
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #6892 — a cURL `progress` callback (rust-lang/cargo) in a trait impl
    // returns `true` on every path because `true` is the "continue the transfer"
    // protocol sentinel mandated by the `Handler` trait, while it does real
    // side-effecting work. The trait contract fixes the return value, so it must
    // not be flagged.
    #[test]
    fn allows_trait_impl_method_returning_sentinel() {
        let src = r#"
impl Handler for Collector {
    fn progress(&mut self, dltotal: f64, dlnow: f64) -> bool {
        if dlnow > dltotal {
            return true;
        }
        self.stats.add(1);
        true
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Negative space for #6892: an *inherent* impl method (`impl Type`, no
    // `trait` field) is not trait-constrained, so its invariant returns are a
    // genuine smell and must still fire.
    #[test]
    fn flags_inherent_impl_invariant_returns() {
        let src = r#"
impl Foo {
    fn bar(&self, x: i32) -> i32 {
        if x > 0 {
            return 0;
        }
        self.side_effect();
        0
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Negative space for #6892: the trait-impl guard keys on the *immediate*
    // enclosing item. A free function declared inside a trait method body is a
    // child of a `block`, not the impl's `declaration_list`, so its own
    // invariant returns still fire — the guard must not misattribute the outer
    // trait impl to a nested local function.
    #[test]
    fn flags_local_fn_inside_trait_method() {
        let src = r#"
impl Handler for Collector {
    fn progress(&mut self) -> bool {
        fn helper(x: i32) -> i32 {
            if x > 0 {
                return 0;
            }
            side_effect();
            0
        }
        helper(1);
        true
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
