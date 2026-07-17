//! rust-no-bool-return-from-fallible backend.
//!
//! Walks `function_item` nodes whose return type is `bool` and whose
//! name suggests an action (verb prefix from a small allowlist). The
//! smell is an action that returns a bare `true` on one path and a bare
//! `false` on another: the operation's success/failure is collapsed into
//! a bool the caller can't get a reason from, instead of `Result<T, E>`.
//!
//! Several shapes are exempted because the bool is legitimate:
//! - Pure predicates (`is_empty`, `has_x`, `contains`): a bool is the
//!   right answer to a question about state.
//! - Atomic fetch/bitwise ops (`fetch_and`, `fetch_or`, `fetch_xor`,
//!   `fetch_nand`, …): the bool is the *previous* atomic value, not a
//!   success flag — these always succeed.
//! - Functions with a leading doc comment phrased as a boolean
//!   question/answer ("Returns `true` if …", "Checks whether …"): the
//!   doc marks a predicate even when the name carries an action prefix
//!   (e.g. seqlock `validate_read`).
//! - Trait-impl methods (`impl Trait for Type`): the signature is fixed
//!   by the trait contract, the implementor can't return `Result`.
//! - Functions whose body tail expression *computes* the bool from a
//!   real value — a comparison (parser progress: `pos() != start`), a
//!   forwarded call return (`HashSet::insert`'s "was it new?"), or a
//!   bool-producing macro tail (`cfg!(...)`, an env-helper macro that
//!   forwards a computed value) — rather than hardcoding a literal. A
//!   `match`/`if` tail counts as computed
//!   when at least one branch body forwards a computed value (e.g.
//!   `match { Some(f) => (f)(x), None => true }`); a `match`/`if` whose
//!   every branch is a bare `true` / `false` is not treated as computed.
//! - Functions whose every boolean-literal return is the *same* constant
//!   (all `true`, or all `false`): with a single possible outcome the bool
//!   is a dispatch tag ("handler recognized the sequence"), not a
//!   success/failure collapse — there is no failure path to surface as a
//!   `Result`. A return forwarding a non-literal value keeps the function
//!   in scope.
//! - Functions whose `bool` is the `Some`/`None` discriminant of an
//!   `Option` match (`match lookup() { Some(i) => { act(i); true } None =>
//!   false }`): the bool reports presence ("was it found and removed?"),
//!   the `BTreeSet::remove` idiom. `Option` carries no error, so the literal
//!   is structural state, not a swallowed failure. A `Result` match
//!   (`Ok`/`Err`) keeps flagging — its `Err` is a genuine failure path.
//! - Total guard-clause predicates: every `return`ed value and the tail is a
//!   *direct* boolean literal — bare `return false;` guards and a literal tail
//!   (`if <cmp> { return false; } … true`), not the `if op() { true } else {
//!   false }` collapse — *and* the body performs no operation whose failure it
//!   could swallow: no `?` (`try_expression`), no `Ok`/`Err` construction or
//!   match pattern, no `.is_ok()`/`.is_err()` call, no discarded call statement
//!   (`persist(x);`). With a provably infallible body there is no error to hoist
//!   into `Result::Err`, so `bool` is correct (generalizes the `Some`/`None`
//!   case to guard clauses). A body that maps a condition onto literals or
//!   swallows an operation keeps flagging — that is the rule's real target.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_trait_impl;

const KINDS: &[&str] = &["function_item"];

const ACTION_PREFIXES: &[&str] = &[
    "save_",
    "delete_",
    "remove_",
    "create_",
    "update_",
    "insert_",
    "parse_",
    "validate_",
    "connect_",
    "send_",
    "write_",
    "load_",
    "execute_",
    "process_",
    "publish_",
    "submit_",
    "commit_",
    "apply_",
    "fetch_",
    "store_",
    "register_",
    "unregister_",
];

const EXEMPT_PREFIXES: &[&str] = &[
    "is_",
    "has_",
    "should_",
    "can_",
    "may_",
    "must_",
    "needs_",
    "contains_",
    "matches_",
    "supports_",
    "accepts_",
];

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
        let source_bytes = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        if !looks_like_action(name) {
            return;
        }
        // Predicate-style names take precedence: an `is_valid()` that
        // returns bool is correct, even if it's also "save_valid".
        if looks_like_predicate(name) {
            return;
        }
        // Atomic fetch/bitwise ops (`fetch_and`, `fetch_or`, `fetch_xor`,
        // `fetch_nand`, …) return the *previous* atomic value, not a
        // success flag — they always succeed. Mirrors
        // `std::sync::atomic::AtomicBool::fetch_*`.
        if is_atomic_fetch_op(name) {
            return;
        }
        // A leading doc comment phrased as a boolean question/answer
        // ("Returns `true` if …", "Checks whether …") marks the function
        // as a predicate even when its name carries an action prefix
        // (e.g. seqlock `validate_read`).
        if has_predicate_doc_comment(node, source_bytes) {
            return;
        }
        let Some(ret_type) = node.child_by_field_name("return_type") else {
            return;
        };
        let Ok(ret_text) = ret_type.utf8_text(source_bytes) else {
            return;
        };
        if ret_text.trim() != "bool" {
            return;
        }
        // A trait-impl method can't change its signature to `Result` —
        // the `bool` is dictated by the trait contract.
        if is_in_trait_impl(node) {
            return;
        }
        // The bool is a genuine computed value (parser-progress
        // comparison, forwarded collection-insert result), not a
        // failure smuggled as a hardcoded `true` / `false`.
        if returns_computed_bool(node) {
            return;
        }
        // A continuation predicate driving a `while` loop
        // (`while self.NAME() {}` / `while NAME() {}`): the bool encodes
        // iteration state (`true` = keep going, `false` = done), not
        // success/failure. The call site proves there is no error path.
        if drives_while_loop(node, name, source_bytes) {
            return;
        }
        // A function whose every boolean-literal return is the *same*
        // constant (all `true`, or all `false`) has a single possible
        // outcome — its `bool` is a dispatch tag, not a success/failure
        // collapse, so there is no failure path to hoist into a `Result`.
        // Both literals must be reachable for the smell to exist; a return
        // forwarding a non-literal value leaves the function in scope.
        if returns_single_constant_bool(node, source_bytes) {
            return;
        }
        // The `bool` is the `Some`/`None` discriminant of an `Option` match
        // (`match lookup() { Some(i) => { act(i); true } None => false }`):
        // it reports presence ("was it found and removed?"), the
        // `BTreeSet::remove` idiom. `Option` has no error to surface as a
        // `Result`, so the literal is structural state, not a swallowed
        // failure. A `Result` match (`Ok`/`Err`) is not exempted here.
        if returns_option_presence_bool(node, source_bytes) {
            return;
        }
        // A total guard-clause predicate: every `return`ed value and the tail is
        // a direct bool literal (`if <cmp> { return false; } … true`) and the
        // body performs no operation whose failure it could swallow. With a
        // provably infallible body there is no error to surface as a `Result`,
        // so the `bool` is correct — the `Some`/`None` case generalized to guard
        // clauses. A condition-to-literal collapse or a swallowed operation
        // still flags below.
        if returns_total_predicate(node, source_bytes) {
            return;
        }
        // mdBook tutorial example code (an ancestor `book.toml`) is
        // documentation, not library API — its simplified examples
        // intentionally return `bool` and are exempt.
        if ctx.project.in_mdbook_project(ctx.path) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-bool-return-from-fallible".into(),
            message: format!(
                "`fn {name}(..) -> bool` — action functions must \
                 return `Result<T, E>` so the caller can see why \
                 the operation failed. Use `Result<(), MyError>` \
                 if there's no success payload."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if the function's body tail expression *computes* its boolean
/// result from a real value rather than hardcoding `true` / `false`.
///
/// The rule's smell is an action that discards an operation's failure
/// and returns a bare literal. When the implicit-return expression is a
/// comparison (`pos() != start` — parser progress) or a directly
/// forwarded call return (`self.set.insert(x)` — was it new?), the bool
/// carries the operation's actual outcome and is the right return type.
fn returns_computed_bool(func: tree_sitter::Node) -> bool {
    let Some(body) = func.child_by_field_name("body") else {
        return false;
    };
    let Some(tail) = block_tail_expression(body) else {
        return false;
    };
    expression_is_computed(tail)
}

/// A block's implicit return is its last named child, provided that child
/// is an expression. A trailing `expr;` statement is wrapped in
/// `expression_statement`; the `;` is a separate `empty_statement`, so a
/// genuine tail statement leaves that `empty_statement` last and is not
/// mistaken for a value. Block-like tail expressions (`match`, `if`) are
/// themselves wrapped in `expression_statement` even without a `;`, so the
/// wrapper is unwrapped to expose the inner expression. Comments are
/// skipped.
fn block_tail_expression(block: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = block.walk();
    let tail = block
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "line_comment" && child.kind() != "block_comment")
        .last()?;
    if tail.kind() == "expression_statement" {
        return tail.named_child(0);
    }
    Some(tail)
}

/// True if `expr` *computes* its boolean value from a real value rather
/// than hardcoding `true` / `false`. A comparison (`pos() != start`) or a
/// forwarded call return (`self.set.insert(x)`, a closure's `(f)(x)`)
/// carries the operation's actual outcome. A `macro_invocation` tail
/// (`cfg!(...)`, `matches!(x, ..)`, an env-helper macro) forwards its
/// result the same way a call does, so it counts as computed too. A
/// `match`/`if` is computed iff at least one branch body is itself
/// computed — an all-literal `match`/`if` (`if ok { true } else { false }`)
/// is the genuine literal-smuggling smell and is not treated as computed.
fn expression_is_computed(expr: tree_sitter::Node) -> bool {
    match expr.kind() {
        "binary_expression"
        | "call_expression"
        | "await_expression"
        | "try_expression"
        | "macro_invocation" => true,
        "match_expression" => match_has_computed_arm(expr),
        "if_expression" => if_has_computed_branch(expr),
        _ => false,
    }
}

/// True if any `match_arm`'s body expression is computed. The arm body is
/// the arm's `value` field, which is either a bare expression or a `block`
/// whose tail is the value.
fn match_has_computed_arm(match_expr: tree_sitter::Node) -> bool {
    let Some(body) = match_expr.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|child| child.kind() == "match_arm")
        .filter_map(|arm| arm.child_by_field_name("value"))
        .any(branch_body_is_computed)
}

/// True if any branch of an `if`/`else` chain computes its tail value. The
/// consequent is a `block`; the alternative is an `else_clause` wrapping a
/// `block` or a chained `else if` (`if_expression`).
fn if_has_computed_branch(if_expr: tree_sitter::Node) -> bool {
    if let Some(cons) = if_expr.child_by_field_name("consequence")
        && branch_body_is_computed(cons)
    {
        return true;
    }
    let Some(alt) = if_expr.child_by_field_name("alternative") else {
        return false;
    };
    match alt.kind() {
        // `else if`: the alternative is directly another `if_expression`.
        "if_expression" => if_has_computed_branch(alt),
        // `else { .. }`: the `else_clause` wraps a `block` or a chained
        // `else if` (`if_expression`).
        "else_clause" => {
            let mut cursor = alt.walk();
            alt.named_children(&mut cursor)
                .any(|child| match child.kind() {
                    "if_expression" => if_has_computed_branch(child),
                    "block" => branch_body_is_computed(child),
                    _ => false,
                })
        }
        _ => false,
    }
}

/// True if a branch body computes its value. A `block` is unwrapped to its
/// tail expression; any other node is the branch value directly (a bare
/// arm body). Recurses through nested `match`/`if`.
fn branch_body_is_computed(body: tree_sitter::Node) -> bool {
    let expr = if body.kind() == "block" {
        match block_tail_expression(body) {
            Some(tail) => tail,
            None => return false,
        }
    } else {
        body
    };
    expression_is_computed(expr)
}

/// Records the boolean-literal outcomes a function can return.
///
/// `saw_indirect` marks that at least one literal was reached by descending
/// into an `if`/`match` expression in *value* position (`if c { true } else {
/// false }`). `returns_single_constant_bool` ignores it; `returns_total_predicate`
/// uses it to tell a guard-clause predicate (bare `return false;` / literal
/// tail) from that condition-to-literal mapping, which is the rule's smell.
#[derive(Default)]
struct BoolReturns {
    saw_true: bool,
    saw_false: bool,
    saw_non_literal: bool,
    saw_indirect: bool,
}

/// True if every boolean value the function can return is the *same*
/// constant literal — all `true`, or all `false`. Such a function has a
/// single possible outcome, so its `bool` is a dispatch tag, not a
/// collapsed success/failure: there is no failure path to surface as a
/// `Result`.
///
/// Both return positions are scanned: the body's tail expression (resolved
/// through `if`/`match` branch tails) and every explicit `return <expr>;`
/// in the function's *own* body. Closures and nested `fn`s are not
/// descended into — a `return` there leaves through its own boundary. A
/// return whose value is not a bool literal (a forwarded call or variable)
/// makes the outcome non-constant, so the function keeps its current
/// behaviour and is not exempted here.
fn returns_single_constant_bool(func: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = func.child_by_field_name("body") else {
        return false;
    };
    let mut returns = BoolReturns::default();
    if let Some(tail) = block_tail_expression(body) {
        classify_return_value(tail, source, &mut returns);
    }
    collect_explicit_returns(body, source, &mut returns);

    if returns.saw_non_literal {
        return false;
    }
    // Exactly one of the two literals is reachable (and at least one is).
    returns.saw_true ^ returns.saw_false
}

/// Which side of an `Option` match an arm pattern selects.
enum OptionArm {
    /// `Some(..)` — a `tuple_struct_pattern` whose path ends in `Some`.
    Present,
    /// `None` — an identifier/path ending in `None`.
    Absent,
}

/// True if the function's `bool` outcome is the `Some`/`None` discriminant of
/// an `Option` match — a found-or-not indicator (the `BTreeSet::remove`
/// idiom: act inside the `Some` arm, report presence as the `bool`). The
/// body's tail is a `match` whose arms are `Some(..)` and `None`, each
/// yielding a `bool` literal (a `Some` arm may run side effects before its
/// literal tail). `Option` carries no error, so the `bool` reports presence,
/// not a swallowed failure — there is no `Result` to offer instead. A
/// `Result` match (`Ok`/`Err`) is deliberately not matched here: its `Err`
/// arm is a genuine failure path the rule should still surface.
fn returns_option_presence_bool(func: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = func.child_by_field_name("body") else {
        return false;
    };
    let Some(tail) = block_tail_expression(body) else {
        return false;
    };
    if tail.kind() != "match_expression" {
        return false;
    }
    let Some(match_body) = tail.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = match_body.walk();
    let mut saw_some = false;
    let mut saw_none = false;
    for arm in match_body
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "match_arm")
    {
        let Some(pattern) = arm.child_by_field_name("pattern") else {
            return false;
        };
        match option_arm_kind(pattern, source) {
            Some(OptionArm::Present) => saw_some = true,
            Some(OptionArm::Absent) => saw_none = true,
            // A non-`Option` arm (`Ok`/`Err`, an enum variant, `_`, a binding)
            // means this is not the presence idiom.
            None => return false,
        }
        let Some(value) = arm.child_by_field_name("value") else {
            return false;
        };
        if !arm_value_is_bool_literal(value) {
            return false;
        }
    }
    saw_some && saw_none
}

/// Classifies a match-arm pattern as `Some(..)` or `None`. Returns the
/// absent `Option` (no classification) for anything else — `Ok`, `Err`,
/// another enum variant, a wildcard `_`, or a bare binding.
fn option_arm_kind(pattern: tree_sitter::Node, source: &[u8]) -> Option<OptionArm> {
    let inner = if pattern.kind() == "match_pattern" {
        pattern.named_child(0)?
    } else {
        pattern
    };
    let text = inner.utf8_text(source).ok()?.trim();
    if let Some(paren) = text.find('(') {
        let head = text[..paren].trim();
        return (head == "Some" || head.ends_with("::Some")).then_some(OptionArm::Present);
    }
    let last = text.rsplit("::").next().unwrap_or(text).trim();
    (last == "None").then_some(OptionArm::Absent)
}

/// True if an arm's `value` resolves to a `bool` literal: a bare `true` /
/// `false`, or a `block` whose tail expression is one (the `Some` arm may run
/// side effects before its literal).
fn arm_value_is_bool_literal(value: tree_sitter::Node) -> bool {
    let expr = if value.kind() == "block" {
        match block_tail_expression(value) {
            Some(tail) => tail,
            None => return false,
        }
    } else {
        value
    };
    expr.kind() == "boolean_literal"
}

/// Classifies a value in return position. `if`/`match` are descended into
/// their branch tails; a `block` resolves to its tail; a bool literal sets
/// the matching flag; anything else is a non-literal outcome.
fn classify_return_value(expr: tree_sitter::Node, source: &[u8], returns: &mut BoolReturns) {
    match expr.kind() {
        "boolean_literal" => match expr.utf8_text(source) {
            Ok("true") => returns.saw_true = true,
            Ok("false") => returns.saw_false = true,
            _ => returns.saw_non_literal = true,
        },
        "if_expression" => classify_if_branches(expr, source, returns),
        "match_expression" => classify_match_arms(expr, source, returns),
        "block" => match block_tail_expression(expr) {
            Some(tail) => classify_return_value(tail, source, returns),
            None => returns.saw_non_literal = true,
        },
        _ => returns.saw_non_literal = true,
    }
}

/// Classifies both branches of an `if`/`else` chain. An `if` without an
/// `else` cannot yield a bool literal on the missing arm, so it counts as a
/// non-literal outcome.
fn classify_if_branches(if_expr: tree_sitter::Node, source: &[u8], returns: &mut BoolReturns) {
    // A literal reached through an `if`/`else` value is not a direct guard-clause
    // return — it is the `if c { true } else { false }` collapse the rule targets.
    returns.saw_indirect = true;
    match if_expr.child_by_field_name("consequence") {
        Some(cons) => classify_return_value(cons, source, returns),
        None => returns.saw_non_literal = true,
    }
    match if_expr.child_by_field_name("alternative") {
        Some(alt) => match alt.kind() {
            // `else if`: the alternative is directly another `if_expression`.
            "if_expression" => classify_if_branches(alt, source, returns),
            // `else { .. }`: the `else_clause` wraps a `block` or a chained
            // `else if` (`if_expression`).
            "else_clause" => {
                let mut cursor = alt.walk();
                for child in alt.named_children(&mut cursor) {
                    match child.kind() {
                        "if_expression" => classify_if_branches(child, source, returns),
                        "block" => classify_return_value(child, source, returns),
                        _ => {}
                    }
                }
            }
            _ => returns.saw_non_literal = true,
        },
        None => returns.saw_non_literal = true,
    }
}

/// Classifies every arm body of a `match` expression.
fn classify_match_arms(match_expr: tree_sitter::Node, source: &[u8], returns: &mut BoolReturns) {
    // A literal reached through a `match` value is not a direct guard-clause
    // return (see `classify_if_branches`).
    returns.saw_indirect = true;
    let Some(body) = match_expr.child_by_field_name("body") else {
        returns.saw_non_literal = true;
        return;
    };
    let mut cursor = body.walk();
    let mut saw_arm = false;
    for arm in body
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "match_arm")
    {
        saw_arm = true;
        match arm.child_by_field_name("value") {
            Some(value) => classify_return_value(value, source, returns),
            None => returns.saw_non_literal = true,
        }
    }
    if !saw_arm {
        returns.saw_non_literal = true;
    }
}

/// Records bool-literal outcomes from every `return <expr>;` in the
/// function's own body. Closures, `async` blocks, and nested `fn`s are
/// skipped so their returns — which leave through their own boundary, not
/// this function's — are not attributed here.
fn collect_explicit_returns(node: tree_sitter::Node, source: &[u8], returns: &mut BoolReturns) {
    match node.kind() {
        "closure_expression" | "async_block" | "function_item" => return,
        "return_expression" => {
            if let Some(value) = node.named_child(0) {
                classify_return_value(value, source, returns);
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_explicit_returns(child, source, returns);
    }
}

/// True if the function is a *total guard-clause predicate*: every value it can
/// return — its tail expression and every `return <expr>;` in its own body — is
/// a **direct** boolean literal (a bare `true` / `false`, not one produced by an
/// `if`/`match` value expression), *and* its body performs no operation whose
/// failure it could be swallowing. A guard-clause predicate (`if <cmp> { return
/// false; } … true`) collapses no operation's failure into its `bool`: with a
/// provably infallible body there is no error to surface as `Result::Err`, so
/// `bool` is the correct return type. This generalizes
/// `returns_option_presence_bool` ("no `Result` to offer instead") from the
/// `Some`/`None` tail idiom to guard clauses.
///
/// Every condition is required, and each guards a distinct true-positive:
/// - `saw_non_literal` — a forwarded/computed return is a real value.
/// - `saw_indirect` — a literal produced by an `if`/`match` *value* is the
///   `if op() { true } else { false }` collapse the rule exists to flag.
/// - `body_swallows_operation` — a `?`, `Ok`/`Err`, `.is_ok()`/`.is_err()`, or a
///   discarded call statement is an operation whose failure is being dropped.
fn returns_total_predicate(func: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = func.child_by_field_name("body") else {
        return false;
    };
    let mut returns = BoolReturns::default();
    if let Some(tail) = block_tail_expression(body) {
        classify_return_value(tail, source, &mut returns);
    }
    collect_explicit_returns(body, source, &mut returns);

    if returns.saw_non_literal
        || returns.saw_indirect
        || !(returns.saw_true || returns.saw_false)
    {
        return false;
    }
    !body_swallows_operation(body, source)
}

/// True if `node`'s subtree performs an operation whose failure a total
/// predicate could be swallowing: the `?` operator (`try_expression`), an
/// `Ok(..)`/`Err(..)` construction or match pattern, a `.is_ok()`/`.is_err()`
/// call, or a bare call/`await` statement whose result is discarded
/// (`persist(x);`). Closures, `async` blocks, and nested `fn`s are not descended
/// into — an operation there belongs to that inner boundary, not this function
/// (mirrors `collect_explicit_returns`).
fn body_swallows_operation(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "closure_expression" | "async_block" | "function_item" => return false,
        "try_expression" => return true,
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function")
                && call_is_fallible(function, source)
            {
                return true;
            }
        }
        "tuple_struct_pattern" => {
            if let Some(type_node) = node.child_by_field_name("type")
                && path_tail_is_result_variant(type_node, source)
            {
                return true;
            }
        }
        "expression_statement" => {
            // A call/`await` performed for effect with its result dropped is an
            // operation that could be failing silently (`persist(x);`). A macro
            // statement (`debug_assert!`, `println!`) is not counted.
            if node
                .named_child(0)
                .is_some_and(|inner| matches!(inner.kind(), "call_expression" | "await_expression"))
            {
                return true;
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| body_swallows_operation(child, source))
}

/// True if a `call_expression`'s `function` operand marks a fallible call: a
/// `.is_ok()`/`.is_err()` method (the callee is a `field_expression` whose
/// `field` is `is_ok`/`is_err`), or an `Ok(..)`/`Err(..)` construction (the
/// callee path ends in `Ok`/`Err`).
fn call_is_fallible(function: tree_sitter::Node, source: &[u8]) -> bool {
    if function.kind() == "field_expression" {
        return function
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok())
            .is_some_and(|f| f == "is_ok" || f == "is_err");
    }
    path_tail_is_result_variant(function, source)
}

/// True if a path node's final `::`-segment names a `Result` variant — `Ok` or
/// `Err`, plain or module-qualified (`Result::Ok`, `std::result::Result::Err`).
fn path_tail_is_result_variant(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    let tail = text.rsplit("::").next().unwrap_or(text).trim();
    tail == "Ok" || tail == "Err"
}

fn looks_like_action(name: &str) -> bool {
    let lower = format!("{}_", name.to_ascii_lowercase());
    ACTION_PREFIXES.iter().any(|p| lower.starts_with(p))
}

/// True if `name` reads as a predicate. An exempt token (`is_`, `has_`,
/// `needs_`, …) counts whether it leads the name or appears as an internal
/// snake_case segment (`update_needs_…`, `…_is_missing`) — the segment is
/// matched via a leading `_`, so `_is_` does not match `_island`. A name
/// ending in a predicate suffix (`_is_missing`, `_is_present`, `_exists`) is
/// likewise a predicate.
fn looks_like_predicate(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if EXEMPT_PREFIXES
        .iter()
        .any(|p| lower.starts_with(p) || lower.contains(&format!("_{p}")))
    {
        return true;
    }
    const PREDICATE_SUFFIXES: &[&str] = &["_is_missing", "_is_present", "_exists"];
    PREDICATE_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

/// True if `name` is an atomic fetch/bitwise op whose `bool` return is the
/// prior atomic value (`fetch`, `fetch_and`, `fetch_or`, `fetch_xor`,
/// `fetch_nand`, …). These mirror `std::sync::atomic::AtomicBool::fetch_*`
/// and always succeed, so the `bool` payload is correct.
fn is_atomic_fetch_op(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "fetch" || lower.starts_with("fetch_")
}

/// True if a doc comment immediately preceding the function reads as a
/// boolean question/answer — i.e. the function is a predicate, not a
/// fallible action, even though its name carries an action prefix.
///
/// In tree-sitter-rust, doc comments (`///`, `/** */`) are `line_comment`
/// / `block_comment` siblings preceding the item, possibly interleaved
/// with `attribute_item`s. Each doc line is normalized (markers and
/// backticks stripped, lowercased) and matched two ways: a `starts_with`
/// against question/answer lead-ins ("returns true", "figure out if", …) so
/// an action that merely mentions "validate" mid-prose still fires, and a
/// `contains` against "return(s) true/false if/when" and "return(s) whether"
/// phrases so a predicate answer phrased mid-sentence ("…and return true if one
/// was loaded", "This will return whether the hints changed") counts.
fn has_predicate_doc_comment(func: tree_sitter::Node, source: &[u8]) -> bool {
    const PREDICATE_DOC_LEADS: &[&str] = &[
        "returns true",
        "returns whether",
        "checks whether",
        "returns false",
        "return true",
        "return false",
        "figure out if",
        "figure out whether",
        "determine if",
        "determine whether",
    ];
    const PREDICATE_DOC_PHRASES: &[&str] = &[
        "return true if",
        "returns true if",
        "return true when",
        "returns true when",
        "return false if",
        "returns false if",
        "return false when",
        "returns false when",
        "return whether",
        "returns whether",
    ];
    let mut sibling = func.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {}
            "line_comment" | "block_comment" => {
                if let Ok(text) = s.utf8_text(source) {
                    let normalized = text
                        .trim_start_matches(['/', '*', '!'])
                        .trim()
                        .replace('`', "")
                        .to_ascii_lowercase();
                    if PREDICATE_DOC_LEADS
                        .iter()
                        .any(|lead| normalized.starts_with(lead))
                        || PREDICATE_DOC_PHRASES
                            .iter()
                            .any(|phrase| normalized.contains(phrase))
                    {
                        return true;
                    }
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `name` is invoked as the condition of a `while` loop anywhere in
/// the same file — the continuation-predicate pattern (`while self.NAME() {}`
/// / `while NAME() {}`). Such a `bool` drives iteration (`true` = continue,
/// `false` = done), so it carries loop state, not an operation's success.
///
/// The whole tree is scanned from the file root because the loop call site can
/// be in a sibling method (`build`) of the same impl, not in `NAME`'s own body.
/// This runs only when the function is otherwise about to be flagged, so the
/// per-flag walk is rare.
fn drives_while_loop(func: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut root = func;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "while_expression"
            && let Some(cond) = node.child_by_field_name("condition")
            && while_condition_calls(cond, name, source)
        {
            return true;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if `cond` is a call whose callee names `name`: either a free call
/// (`NAME()` → `function` is an `identifier`) or a method/path call
/// (`self.NAME()` / `Type::NAME()` → `function` is a `field_expression` whose
/// `field` is `NAME`, or a `scoped_identifier` ending in `NAME`).
fn while_condition_calls(cond: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if cond.kind() != "call_expression" {
        return false;
    }
    let Some(function) = cond.child_by_field_name("function") else {
        return false;
    };
    let callee = match function.kind() {
        "identifier" => function,
        "field_expression" => match function.child_by_field_name("field") {
            Some(field) => field,
            None => return false,
        },
        "scoped_identifier" => match function.child_by_field_name("name") {
            Some(seg) => seg,
            None => return false,
        },
        _ => return false,
    };
    callee.utf8_text(source).is_ok_and(|text| text == name)
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

    /// Run on a `.rs` file inside a temp mdBook project: a `book.toml` marker at
    /// the project root and the source at `src/<rel_path>`, so the
    /// `in_mdbook_project` ancestor-walk finds the marker.
    fn run_in_mdbook(rel_path: &str, source: &str) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("book.toml"), "[book]\ntitle = \"Guide\"\n").unwrap();
        let src_path = dir.path().join("src").join(rel_path);
        fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    #[test]
    fn flags_save_returning_bool() {
        assert_eq!(
            run_on("fn save_user(u: &User) -> bool { if persist(u) { true } else { false } }").len(),
            1
        );
    }

    #[test]
    fn flags_parse_returning_bool() {
        assert_eq!(
            run_on("fn parse_config(s: &str) -> bool { if scan(s) { true } else { false } }").len(),
            1
        );
    }

    #[test]
    fn allows_save_returning_result() {
        assert!(run_on("fn save_user(u: &User) -> Result<(), MyError> { Ok(()) }").is_empty());
    }

    /// Regression for #5846 (leudz/shipyard
    /// `guide/master/src/going-further/custom_views_original.rs`): a fallible
    /// action returning `bool` inside an mdBook documentation project (an
    /// ancestor `book.toml`) is tutorial example code, not library API, so it
    /// is exempt.
    #[test]
    fn allows_bool_action_in_mdbook_example() {
        let source =
            "fn process_events(&mut self) -> bool { side_effect(); if step() { true } else { false } }";
        assert!(run_in_mdbook("going-further/custom_views_original.rs", source).is_empty());
    }

    /// The same construct in an ordinary library file (no `book.toml` ancestor)
    /// stays flagged — the exemption is mdBook-scoped, not universal.
    #[test]
    fn flags_bool_action_outside_mdbook() {
        let source =
            "fn process_events(&mut self) -> bool { side_effect(); if step() { true } else { false } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_predicate_is_valid() {
        assert!(run_on("fn is_valid(s: &str) -> bool { true }").is_empty());
    }

    #[test]
    fn allows_predicate_has_permission() {
        assert!(run_on("fn has_permission(u: &User) -> bool { true }").is_empty());
    }

    #[test]
    fn does_not_flag_unrelated_function() {
        assert!(run_on("fn add(a: i32, b: i32) -> i32 { a + b }").is_empty());
    }

    // --- #1733: trait-impl methods (signature fixed by the contract) ---

    #[test]
    fn allows_validate_in_trait_impl() {
        let src = "\
            impl biome_deserialize::DeserializableValidator for FilenameCases {\n\
                fn validate(&mut self, ctx: &mut C, name: &str, range: R) -> bool {\n\
                    if !self.allow_export && self.cases.is_empty() {\n\
                        ctx.report(d);\n\
                        false\n\
                    } else {\n\
                        true\n\
                    }\n\
                }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_validate_in_inherent_impl() {
        // An inherent (non-trait) impl can freely return `Result`, so the
        // bool smell still applies.
        let src = "impl Foo { fn validate_thing(&self) -> bool { if ok() { true } else { false } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #1733: parser-progress comparison return ---

    #[test]
    fn allows_parser_progress_comparison() {
        let src = "\
            pub(crate) fn parse_css_generic_component_value_list(p: &mut TailwindParser) -> bool {\n\
                let start = p.source().position();\n\
                CssGenericComponentValueList.parse_list(p);\n\
                p.source().position() != start\n\
            }";
        assert!(run_on(src).is_empty());
    }

    // --- #1733: forwarded collection-insert result ---

    #[test]
    fn allows_forwarded_insert_result() {
        let src = "\
            fn insert_watched_folder(&self, path: Utf8PathBuf) -> bool {\n\
                self.watched_folders.pin().insert(path)\n\
            }";
        assert!(run_on(src).is_empty());
    }

    // --- #1733: negative space — genuine fallible action still flagged ---

    #[test]
    fn flags_genuine_action_swallowing_failure() {
        // Success and failure are collapsed into `true` / `false` literals
        // instead of a `Result` — exactly the smell.
        let src = "fn save_user(u: &User) -> bool { if db.write(u) { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_genuine_action_with_literal_branches() {
        let src = "fn save_user(u: &User) -> bool { if ok { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #3965: match/if tail whose arm forwards a computed bool ---

    #[test]
    fn allows_validate_match_forwarding_closure_bool() {
        // async-graphql `ScalarType::validate` — the `Some` arm forwards a
        // user closure's `bool`; `None` legitimately means "valid".
        let src = "\
            pub(crate) fn validate(&self, value: &Value) -> bool {\n\
                match &self.validator {\n\
                    Some(validator) => (validator)(value),\n\
                    None => true,\n\
                }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_if_tail_forwarding_call_in_one_branch() {
        let src = "\
            fn validate_thing(&self, cond: bool) -> bool {\n\
                if cond { self.compute() } else { true }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    // --- #3965: negative space — all-literal branch bodies still fire ---

    #[test]
    fn flags_match_with_all_literal_arms() {
        let src = "fn validate_state(&self, x: State) -> bool { match x { A => true, B => false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #1479: atomic `fetch_*` ops return the previous value, not success ---

    #[test]
    fn allows_atomic_fetch_and() {
        // crossbeam-utils AtomicCell<bool>::fetch_and — the bool is the
        // previous atomic value; exempted by the `fetch_*` name class,
        // which runs before `returns_computed_bool`.
        let src = "\
            pub fn fetch_and(&self, val: bool) -> bool {\n\
                atomic! {\n\
                    bool, _a,\n\
                    { a.fetch_and(val, Ordering::AcqRel) },\n\
                    { let old = *value; *value &= val; old }\n\
                }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_atomic_fetch_or() {
        // Literal tail so the exemption comes from the name class, not
        // from `returns_computed_bool`.
        let src = "pub fn fetch_or(&self, val: bool) -> bool { side_effect(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_atomic_fetch_xor() {
        let src = "pub fn fetch_xor(&self, val: bool) -> bool { side_effect(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_atomic_fetch_nand() {
        let src = "pub fn fetch_nand(&self, val: bool) -> bool { side_effect(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_fetch_returning_prior_bool() {
        let src = "pub fn fetch(&self, val: bool) -> bool { side_effect(); true }";
        assert!(run_on(src).is_empty());
    }

    // --- #1479: predicate functions whose doc says "Returns `true`" ---

    #[test]
    fn allows_validate_read_with_returns_true_doc() {
        // crossbeam-utils SeqLock::validate_read — a pure predicate
        // ("Returns `true` if the current stamp is equal to `stamp`.").
        let src = "\
            /// Returns `true` if the current stamp is equal to `stamp`.\n\
            pub(crate) fn validate_read(&self, stamp: (usize, usize)) -> bool {\n\
                some_side_effect();\n\
                true\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_check_prefix_with_returns_whether_doc() {
        let src = "\
            /// Returns whether the cache is consistent.\n\
            fn check_consistency(&self) -> bool { do_thing(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_validate_prefix_with_checks_whether_doc() {
        let src = "\
            /// Checks whether the signature is well-formed.\n\
            fn validate_signature(&self) -> bool { do_thing(); true }";
        assert!(run_on(src).is_empty());
    }

    // --- #1479: negative space — `validate_`/`check_` without a predicate
    // doc comment is still a genuine fallible action and must fire ---

    #[test]
    fn flags_validate_action_without_predicate_doc() {
        let src = "fn validate_config(&self) -> bool { do_thing(); if ok() { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #3931: imperative-singular predicate doc ("Return `true`") ---

    #[test]
    fn allows_remove_with_imperative_return_true_doc() {
        // bevy GraphMap::remove_single_edge — the bool answers "was the
        // element there?", documented imperatively.
        let src = "\
            /// Return `true` if it did exist.\n\
            fn remove(&mut self, k: K) -> bool { self.inner.remove(&k); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_remove_edge_with_imperative_return_false_doc() {
        // bevy GraphMap::remove_edge — `Return \`false\` if the edge didn't exist.`
        let src = "\
            /// Return `false` if the edge didn't exist.\n\
            pub fn remove_edge(&mut self, a: N, b: N) -> bool { do_thing(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_validate_action_with_unrelated_doc() {
        let src = "\
            /// Validates the config and persists the result.\n\
            fn validate_config(&self) -> bool { do_thing(); if ok() { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #3716: predicate signal in an internal name segment / suffix ---

    #[test]
    fn allows_action_prefix_with_internal_predicate_segment() {
        // Name leads with `update_` but carries `_needs_` and ends `_is_missing`.
        let src = "\
            fn update_needs_adjustment_as_edits_symbolic_target_is_missing(x: u8) -> bool {\n\
                let a = q(x);\n\
                if a { return false; }\n\
                !w()\n\
            }";
        assert!(run_on(src).is_empty());
    }

    // --- #3716: broadened doc lead-ins and mid-doc predicate phrasing ---

    #[test]
    fn allows_figure_out_if_doc_lead() {
        let src = "\
            /// Figure out if x. If so, return true.\n\
            fn process_thing() -> bool { check() }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_mid_doc_return_true_if_phrase() {
        let src = "\
            /// load a new index, and return true if one was indeed loaded\n\
            fn load_next_index() -> bool { do_load() }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_determine_whether_doc_lead() {
        let src = "\
            /// Determine whether the cache is warm.\n\
            fn refresh_cache_state() -> bool { warm() }";
        assert!(run_on(src).is_empty());
    }

    // --- #3716: negative space — genuine actions still flagged ---

    #[test]
    fn flags_genuine_action_no_predicate_name_or_doc() {
        let src = "fn save_file(p: &str) -> bool { if write(p) { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn island_segment_not_mistaken_for_is_segment() {
        // `_island` must not match the `_is_` internal-segment check.
        assert!(!looks_like_predicate("parse_island_data"));
        // An action prefix + `island` still flags (it is not a predicate).
        let src = "fn create_island_index() -> bool { if build() { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #4701: continuation-predicate driving a `while` loop ---

    #[test]
    fn allows_method_driving_while_loop() {
        // georust/geo `MonoPolyBuilder::process_next_pt` — the bool encodes
        // iteration state (`true` = continue, `false` = done), not success.
        // The `while self.process_next_pt() {}` call site proves it is a
        // continuation predicate with no error path.
        let src = "\
            impl Builder {\n\
                pub fn build(mut self) -> Vec<MonoPoly> {\n\
                    while self.process_next_pt() {}\n\
                    self.outputs\n\
                }\n\
                fn process_next_pt(&mut self) -> bool { if more() { return true; } false }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_free_fn_driving_while_loop() {
        // `while advance_state() {}` — bare-identifier call site.
        let src = "\
            fn drive() { while advance_state() {} }\n\
            fn advance_state() -> bool { if more() { return true; } false }";
        assert!(run_on(src).is_empty());
    }

    // --- #4701: negative space — a `-> bool` action with NO while-condition
    // caller stays flagged ---

    #[test]
    fn flags_action_called_in_if_not_while() {
        // `if save_config() {}` is not a continuation predicate.
        let src = "\
            fn setup() { if save_config() { log(); } }\n\
            fn save_config(&self) -> bool { if write() { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_action_with_no_caller() {
        let src = "fn save_config(&self) -> bool { if write() { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #6606: a function whose every bool-literal return is the same
    // constant has one possible outcome (a dispatch tag), not a
    // success/failure collapse, and is not flagged ---

    #[test]
    fn allows_action_always_returning_true() {
        // sharkdp/bat `VisibleScreen::update_with_sgr` — recognizes and
        // consumes the sequence, always succeeding: the tail is a constant
        // `true`, never `false`, so there is no failure path.
        let src = "fn update_with_sgr(&mut self, parameters: &str) -> bool { handle(parameters); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_action_always_returning_false() {
        // sharkdp/bat `VisibleScreen::update_with_unsupported` — always
        // returns `false` ("not recognized"); a single constant cannot
        // encode success vs. failure.
        let src =
            "fn update_with_unsupported(&mut self, sequence: &str) -> bool { self.buf.push_str(sequence); false }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_action_with_branches_but_constant_true_tail() {
        // sharkdp/bat `VisibleScreen::update_with_hyperlink` — the `if`/`else`
        // is in statement position (both arms only run side effects); the
        // function's single return value is the trailing constant `true`.
        let src = "\
            fn update_with_hyperlink(&mut self, sequence: &str) -> bool {\n\
                if sequence == \"8;;\" { self.link.clear(); }\n\
                else { self.link.clear(); self.link.push_str(sequence); }\n\
                true\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_action_all_branches_same_constant() {
        // Every reachable bool literal is `true`, so the outcome is constant.
        let src = "fn update_state(&mut self, x: u8) -> bool { if x > 0 { true } else { true } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_action_with_reachable_true_and_false_returns() {
        // A genuine fallible action: `false` on bad input, `true` on success.
        // Both literals are reachable, so the bool collapses success/failure.
        let src = "fn save_state(&mut self, x: &str) -> bool { if x.is_empty() { return false; } persist(x); true }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_action_mixing_literal_tail_and_computed_return() {
        // A non-literal (forwarded) return makes the outcome non-constant, so
        // the function keeps its current behaviour and stays flagged.
        let src = "fn save_state(&mut self, x: &str) -> bool { if x.is_empty() { return self.try_save(x); } true }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #6607: the bool is the Some/None discriminant of an Option match
    // (the `BTreeSet::remove` "found and removed?" idiom) — Option carries no
    // error, so it is structural state, not a swallowed failure ---

    #[test]
    fn allows_remove_with_option_presence_match() {
        // ajeetdsouza/zoxide `db/mod.rs` `remove` — `position()` yields an
        // `Option`; the `Some` arm removes and reports `true`, the `None` arm
        // reports `false` ("was not present"). No `Result` to offer instead.
        let src = "\
            pub fn remove(&mut self, path: impl AsRef<str>) -> bool {\n\
                match self.dirs().iter().position(|dir| dir.path == path.as_ref()) {\n\
                    Some(idx) => {\n\
                        self.swap_remove(idx);\n\
                        true\n\
                    }\n\
                    None => false,\n\
                }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_remove_with_option_presence_match_inverted() {
        // `None => true` / `Some => false` is still a presence indicator.
        let src = "\
            fn remove_entry(&mut self, k: K) -> bool {\n\
                match self.find(k) { None => true, Some(i) => { self.drop(i); false } }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_result_okerr_match_collapsing_error() {
        // A `Result` match collapses the `Err` into a bare `false`: the error
        // path is real and must be surfaced as `Result`, so it still flags.
        // This proves the exemption is `Option`-specific, not any 2-arm match.
        let src = "\
            fn save_record(&mut self, r: &Record) -> bool {\n\
                match self.db.write(r) { Ok(_) => true, Err(_) => false }\n\
            }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #7268: a documented return contract phrased "return(s) whether X"
    // mid-sentence marks a predicate regardless of the sentence position ---

    #[test]
    fn allows_update_with_mid_sentence_return_whether_doc() {
        // alacritty `update_highlighted_hints` — the doc states it returns
        // *whether* the highlighted hints changed (a dirty flag); the phrase
        // sits mid-sentence, not at the line start, so only the `contains`
        // match catches it. The tail is a non-literal variable, so neither the
        // computed nor single-constant exemption applies — the doc alone saves it.
        let src = "\
            /// Update the mouse/vi mode cursor hint highlighting.\n\
            ///\n\
            /// This will return whether the highlighted hints changed.\n\
            pub fn update_highlighted_hints(&mut self) -> bool {\n\
                let dirty = recompute();\n\
                dirty\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_update_with_returns_whether_phrase() {
        // "returns whether" mid-sentence (not at the line start) is likewise a
        // documented predicate contract.
        let src = "\
            /// Refreshes the cache and returns whether it was dirty.\n\
            fn update_cache(&mut self) -> bool { let dirty = refresh(); dirty }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_update_action_without_predicate_doc() {
        // Same action prefix, no predicate doc phrasing, both bool literals
        // reachable: a genuine fallible action that must still flag.
        let src = "\
            /// Updates the config.\n\
            fn update_config(&mut self) -> bool { if write() { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #7563: a total guard-clause predicate — every return/tail is a bool
    // literal (mixed `true`/`false`) and the body has no fallible construct —
    // has no error to hoist into a `Result`, so `bool` is correct ---

    #[test]
    fn allows_total_guard_clause_predicate() {
        // meilisearch `ListFields::apply_filter` — a pure total predicate:
        // every `return`/tail is a bool literal, the guards are pure
        // comparisons, and the body has no `?`/`Ok`/`Err`/`.is_err()`.
        let src = "\
            fn apply_filter(&self, field: &Field) -> bool {\n\
                if let Some(filter) = &self.filter {\n\
                    if let Some(patterns) = &filter.attribute_patterns {\n\
                        if matches!(patterns.match_str(field.name), PatternMatch::NoMatch) { return false; }\n\
                    }\n\
                    if let Some(displayed) = &filter.displayed {\n\
                        if *displayed != field.displayed.enabled { return false; }\n\
                    }\n\
                    return true;\n\
                }\n\
                true\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_condition_to_literal_collapse_despite_pure_condition() {
        // Direct guard-clause returns are exempt, but `if <cond> { true } else
        // { false }` maps a condition straight onto literals — the collapse the
        // rule targets — so it still flags even with a pure condition and no
        // operation in the body. Only bare `return <literal>;` guards qualify.
        let src = "fn apply_rule(&self, x: &str) -> bool { if x.is_empty() { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #7563: negative space — bool-literal guards but the body swallows an
    // operation, so it keeps flagging (the rule's true target) ---

    #[test]
    fn flags_total_shape_swallowing_try_operator() {
        // Both literals reachable via direct guards and a `?` in the body: the
        // error is being discarded into the `bool`, exactly the smell.
        let src = "\
            fn apply_write(&self, x: &str) -> bool {\n\
                if x.is_empty() { return false; }\n\
                self.flush()?;\n\
                true\n\
            }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_total_shape_swallowing_is_err() {
        // `.is_err()` in a guard condition swallows the `Result` — the operation
        // is in condition position, so only the fallibility-marker check catches
        // it, not the discarded-statement one.
        let src = "\
            fn apply_config(&self, x: &str) -> bool {\n\
                if x.is_empty() { return false; }\n\
                if self.load().is_err() { return false; }\n\
                true\n\
            }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_total_shape_if_let_err_guard() {
        // An `Err(_)` match pattern in a guard proves a real failure path even
        // though every return is a direct literal — the pattern check is the
        // load-bearing detector here.
        let src = "\
            fn apply_record(&self, x: &str) -> bool {\n\
                if x.is_empty() { return false; }\n\
                if let Err(_) = self.db.write() { return false; }\n\
                true\n\
            }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_total_shape_constructing_err() {
        // `Err(..)` construction (bound, not a discarded statement) is a
        // fallible marker: only the construction check catches it.
        let src = "\
            fn apply_event(&self, x: &str) -> bool {\n\
                if x.is_empty() { return false; }\n\
                let _pending = Err(x);\n\
                true\n\
            }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_total_shape_swallowing_discarded_call() {
        // Direct-literal guards but a discarded call statement (`persist(x);`):
        // an operation performed for effect whose failure is dropped.
        let src = "\
            fn apply_change(&self, x: &str) -> bool {\n\
                if x.is_empty() { return false; }\n\
                persist(x);\n\
                true\n\
            }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #7642: a `macro_invocation` tail forwards a computed bool the same way
    // a `call_expression` tail does — it hardcodes no literal, so it is not the
    // success/failure collapse the rule targets ---

    #[test]
    fn allows_macro_invocation_tail_forwarding_bool() {
        // quickwit `search_job_placer.rs` `load_estimation_disabled` — the whole
        // body is a single macro tail that reads a cached env var and forwards a
        // computed `bool`; there is no `true`/`false` literal to hoist.
        let src = "\
            fn load_estimation_disabled() -> bool {\n\
                get_bool_from_env_cached!(\"QW_DISABLE_LOAD_ESTIMATION\", false)\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_matches_macro_tail() {
        // `matches!(...)` produces the bool directly; the action-prefixed name
        // (`process_`) is matched only because of the prefix list.
        let src = "fn process_frame(&self, x: &Frame) -> bool { matches!(x, Frame::Data(_)) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_cfg_macro_tail() {
        let src = "fn load_feature_flag() -> bool { cfg!(feature = \"fast\") }";
        assert!(run_on(src).is_empty());
    }

    // --- #7642: negative space — a macro in the body does not exempt a function
    // that still collapses success/failure onto bool literals ---

    #[test]
    fn flags_literal_collapse_despite_macro_in_condition() {
        // The tail is the `if { .. } else { .. }` literal collapse, not a macro;
        // a `matches!` guard in the *condition* doesn't make the tail computed.
        let src = "\
            fn load_config(&self, x: &str) -> bool {\n\
                if matches!(x, \"\") { false } else { true }\n\
            }";
        assert_eq!(run_on(src).len(), 1);
    }
}
