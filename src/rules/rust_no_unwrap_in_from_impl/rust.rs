//! rust-no-unwrap-in-from-impl backend.
//!
//! Walks `impl_item` nodes implementing the `From` trait itself — its
//! `trait` field is `From<...>` or a qualified path `…::From<...>`
//! (so `impl From<X> for Y` and `impl<T> From<X<T>> for Y<T>`) — and
//! scans the impl body for `.unwrap()` / `.expect()` method calls.
//! Traits whose name merely begins with `From` (`FromRequest`,
//! `FromRequestParts`, `FromStr`, `FromIterator`, …) are unrelated
//! fallible traits and are not matched.
//! `TryFrom` impls are not flagged — there, fallibility is part of
//! the contract. A `.unwrap()` / `.expect()` under a
//! `#[cfg(debug_assertions)]` gate is also skipped: it compiles out in
//! release builds, so it is a debug-only invariant check (the equivalent
//! of `debug_assert!`), not a release failure path.
//! A `.expect("…")` whose message documents an infallible invariant (it
//! contains "invariant" or "unreachable") is also skipped: the author is
//! asserting a guaranteed condition (such as a validated newtype's inner
//! value), not handling a real failure path.
//! A `.unwrap()` / `.expect()` whose receiver is `NonZero*::new(<nonzero
//! integer literal>)` is also skipped: `NonZero*::new(n)` returns `None`
//! only when `n == 0`, so a non-zero literal makes the result statically
//! `Some` and the unwrap cannot panic.
//! A `.unwrap()` / `.expect()` whose receiver is `<Type>::try_from(<ident>)`
//! is also skipped when `<ident>` is the scrutinee of an enclosing `match`
//! arm that has already matched a specific variant (the arm pattern is
//! neither `_` nor a plain binding identifier). This is a pragmatic exemption,
//! not a proof: the arm narrows the scrutinee to one variant, for which a
//! variant-to-variant `try_from` is conventionally total (the common shape of
//! converting between two representations of the same enum). The rule has no
//! type resolution, so it cannot confirm the `TryFrom` impl is total — it
//! accepts a lint false-negative for this idiom rather than the false-positive
//! it produced before.
//! A `.unwrap()` / `.expect()` whose receiver is a write/serialize into an
//! in-memory `Vec<u8>` / `String` buffer is also skipped: `std::io::Write` for
//! `Vec<u8>` and `std::fmt::Write` for `String` never return `Err` (they just
//! grow the heap buffer), so the unwrap on `buf.write_all(…)`,
//! `x.serialize(&mut buf)`, or `write!(&mut buf, …)` — where `buf` is a local
//! `Vec`/`String` — cannot panic at runtime.
//! A `.unwrap()` / `.expect()` whose receiver is `<recv>.downcast::<T>()`
//! (or `downcast_ref`/`downcast_mut`) is also skipped when the call sits in the
//! consequence of an enclosing `if <recv>.is::<T>()` guard with the same
//! receiver and the same type `T`: `Any::downcast` succeeds exactly when
//! `is::<T>()` is true, so the type check proves the downcast cannot fail. A
//! mismatched type or receiver is still flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_under_cfg_debug_assertions, local_let_binds_buffer};

const KINDS: &[&str] = &["impl_item"];

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
        // The trait being implemented sits in the `trait` field.
        // For `impl From<X> for Y`, the field's text starts with `From`.
        // We must NOT match `TryFrom` — same prefix, different contract.
        let Some(trait_node) = node.child_by_field_name("trait") else {
            return;
        };
        let Ok(trait_text) = trait_node.utf8_text(source_bytes) else {
            return;
        };
        if !is_from_impl(trait_text) {
            return;
        }
        // Walk the impl body looking for `.unwrap()` / `.expect()`.
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        collect_unwraps_in(body, source_bytes, ctx, diagnostics);
    }
}

/// True if the trait reference is the `From` trait itself (NOT `TryFrom<...>`).
fn is_from_impl(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.starts_with("TryFrom") {
        return false;
    }
    // Only the `From` trait itself: it's generic, so the trait-field text is
    // `From<...>` or a qualified `path::From<...>`. `FromRequest`, `FromStr`,
    // `FromIterator`, … have extra characters before `<`, so they don't match.
    trimmed.starts_with("From<") || trimmed.contains("::From<")
}

fn collect_unwraps_in(
    body: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && let Some(function) = node.child_by_field_name("function")
            && function.kind() == "field_expression"
            && let Some(field) = function.child_by_field_name("field")
            && let Ok(field_text) = field.utf8_text(source)
            && (field_text == "unwrap" || field_text == "expect")
            // A `#[cfg(debug_assertions)]`-gated statement compiles out in
            // release builds, so its `.unwrap()` is a debug-only invariant
            // check (like `debug_assert!`), not a release failure path.
            && !is_under_cfg_debug_assertions(node, source)
            // A `.expect("…")` whose message documents an infallible invariant
            // asserts a guaranteed condition, not a real failure path.
            && !expect_documents_invariant(node, source)
            // `NonZero*::new(<nonzero literal>)` is statically `Some`, so the
            // unwrap cannot panic — it is provably infallible.
            && !is_infallible_nonzero_new(function, source)
            // `<Type>::try_from(<ident>)` where `<ident>` is the scrutinee of an
            // enclosing match arm matching a specific variant: pragmatic exemption
            // for the conventionally-total variant-to-variant conversion idiom.
            && !is_variant_discriminated_try_from(node, function, source)
            // A write/serialize into an in-memory `Vec<u8>`/`String` buffer is
            // infallible (the std `io::Write`/`fmt::Write` impls never `Err`).
            && !is_infallible_buffer_write(function, source)
            // A `<recv>.downcast::<T>().unwrap()` dominated by an enclosing
            // `if <recv>.is::<T>()` guard (same receiver, same type) cannot
            // fail — the type check proves the downcast succeeds.
            && !is_guarded_downcast_unwrap(node, function, source)
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-unwrap-in-from-impl".into(),
                message: format!(
                    "`.{field_text}()` inside a `From` impl breaks the \
                     infallibility contract. Switch the impl to `TryFrom` \
                     so callers can handle the failure mode."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// True when a `.expect("…")` carries a message documenting an infallible
/// invariant (it contains "invariant" or "unreachable"), i.e. an assertion of a
/// guaranteed condition (such as a validated newtype's inner value) rather than
/// a real failure path. A bare `.unwrap()` (no message) never matches.
fn expect_documents_invariant(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let Ok(args_text) = args.utf8_text(source) else {
        return false;
    };
    let lower = args_text.to_ascii_lowercase();
    lower.contains("invariant") || lower.contains("unreachable")
}

/// True when the `.unwrap()`/`.expect()` receiver is `NonZero*::new(<nonzero
/// integer literal>)` — statically `Some`, so the unwrap cannot panic.
/// `field_expr` is the `<receiver>.unwrap` field_expression.
fn is_infallible_nonzero_new(field_expr: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(receiver) = field_expr.child_by_field_name("value") else {
        return false;
    };
    if receiver.kind() != "call_expression" {
        return false;
    }
    let Some(func) = receiver.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "scoped_identifier" {
        return false;
    }
    // function name must be `new`
    if func.child_by_field_name("name").and_then(|n| n.utf8_text(source).ok()) != Some("new") {
        return false;
    }
    // the type segment (last path component) must start with `NonZero`
    let Some(path) = func
        .child_by_field_name("path")
        .and_then(|n| n.utf8_text(source).ok())
    else {
        return false;
    };
    let ty = path.rsplit("::").next().unwrap_or(path);
    if !ty.starts_with("NonZero") {
        return false;
    }
    // single argument must be a non-zero integer literal
    let Some(args) = receiver.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    let Some(arg) = args.named_children(&mut cursor).next() else {
        return false;
    };
    is_nonzero_int_literal(arg, source)
}

/// True when `node` is an integer literal (optionally negated) whose value is
/// not zero. Conservative: returns false for non-literals or anything it can't
/// confidently classify as non-zero.
fn is_nonzero_int_literal(node: tree_sitter::Node, source: &[u8]) -> bool {
    // peel a unary minus: `-1`
    let lit = if node.kind() == "unary_expression" {
        match node.named_child(0) {
            Some(n) => n,
            None => return false,
        }
    } else {
        node
    };
    if lit.kind() != "integer_literal" {
        return false;
    }
    let Ok(text) = lit.utf8_text(source) else {
        return false;
    };
    // strip `_` separators and a trailing type suffix (i8/u64/usize/…)
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    let cleaned = cleaned.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    // strip a radix prefix and parse the magnitude; non-zero iff some digit != '0'
    let body = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
        .or_else(|| cleaned.strip_prefix("0o"))
        .or_else(|| cleaned.strip_prefix("0O"))
        .or_else(|| cleaned.strip_prefix("0b"))
        .or_else(|| cleaned.strip_prefix("0B"))
        .unwrap_or(cleaned);
    !body.is_empty() && body.bytes().any(|b| b != b'0')
}

/// True when the `.unwrap()`/`.expect()` receiver is a write/serialize into an
/// in-memory `Vec<u8>`/`String` buffer, whose std `io::Write`/`fmt::Write` impls
/// never return `Err`. `field_expr` is the `<receiver>.unwrap` field_expression.
///
/// Two shapes are recognized, both requiring the buffer to be a local `Vec`/
/// `String`/`vec![]` binding in the enclosing scope:
///   - a method/function call passing the buffer by `&mut`:
///     `x.serialize(&mut buf).unwrap()`, `buf.write_all(b"…").unwrap()`;
///   - a `write!`/`writeln!` macro writing into the buffer:
///     `write!(&mut buf, "…").unwrap()`.
fn is_infallible_buffer_write(field_expr: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(receiver) = field_expr.child_by_field_name("value") else {
        return false;
    };
    match receiver.kind() {
        "call_expression" => call_writes_to_buffer(receiver, source),
        "macro_invocation" => macro_writes_to_buffer(receiver, source),
        _ => false,
    }
}

/// True when `call` writes into a local `Vec`/`String` buffer: either an
/// argument is `&mut <buf>`, or the method receiver is `<buf>` and the method is
/// a known `Write` method (`write`/`write_all`/`write_fmt`).
fn call_writes_to_buffer(call: tree_sitter::Node, source: &[u8]) -> bool {
    // Shape 1: any argument is `&mut <buffer-local>`.
    if let Some(args) = call.child_by_field_name("arguments") {
        let mut cursor = args.walk();
        for arg in args.named_children(&mut cursor) {
            if let Some(name) = mut_ref_buffer_ident(arg, source)
                && local_let_binds_buffer(call, name, source)
            {
                return true;
            }
        }
    }
    // Shape 2: `<buffer-local>.write_all(…)` / `.write(…)` / `.write_fmt(…)`.
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "field_expression" {
        return false;
    }
    let method = function
        .child_by_field_name("field")
        .and_then(|n| n.utf8_text(source).ok());
    if !matches!(method, Some("write" | "write_all" | "write_fmt")) {
        return false;
    }
    let Some(method_receiver) = function.child_by_field_name("value") else {
        return false;
    };
    if method_receiver.kind() != "identifier" {
        return false;
    }
    let Ok(name) = method_receiver.utf8_text(source) else {
        return false;
    };
    local_let_binds_buffer(call, name, source)
}

/// True when `mac` is a `write!`/`writeln!` invocation whose first token group
/// writes into a local `Vec`/`String` buffer passed as `&mut <buf>`.
fn macro_writes_to_buffer(mac: tree_sitter::Node, source: &[u8]) -> bool {
    let name = mac
        .child_by_field_name("macro")
        .and_then(|n| n.utf8_text(source).ok());
    if !matches!(name, Some("write" | "writeln")) {
        return false;
    }
    // The token tree holds the raw args; find the first `&mut <ident>` and check
    // it resolves to a buffer local. `write!`'s first arg is the writer.
    let mut cursor = mac.walk();
    for child in mac.named_children(&mut cursor) {
        if child.kind() != "token_tree" {
            continue;
        }
        if let Some(name) = first_mut_ref_ident_in_tokens(child, source) {
            return local_let_binds_buffer(mac, name, source);
        }
    }
    false
}

/// If `arg` is `&mut <ident>` (a `reference_expression` with the `mut` mutable
/// specifier over a plain identifier), return the identifier's text.
fn mut_ref_buffer_ident<'a>(arg: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if arg.kind() != "reference_expression" {
        return None;
    }
    if !arg
        .utf8_text(source)
        .map(|t| t.trim_start().starts_with("&mut"))
        .unwrap_or(false)
    {
        return None;
    }
    let value = arg.child_by_field_name("value")?;
    if value.kind() != "identifier" {
        return None;
    }
    value.utf8_text(source).ok()
}

/// Scan a macro `token_tree` for the first `& mut <identifier>` token sequence
/// and return the identifier's text. Macro contents are unparsed tokens, so this
/// walks the raw `&`, `mut`, identifier token run.
fn first_mut_ref_ident_in_tokens<'a>(
    token_tree: tree_sitter::Node,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut cursor = token_tree.walk();
    let children: Vec<_> = token_tree.children(&mut cursor).collect();
    for window in children.windows(3) {
        let [amp, mut_kw, ident] = window else {
            continue;
        };
        if amp.utf8_text(source).ok() == Some("&")
            && mut_kw.utf8_text(source).ok() == Some("mut")
            && ident.kind() == "identifier"
        {
            return ident.utf8_text(source).ok();
        }
    }
    None
}

/// True when the `.unwrap()`/`.expect()` receiver is `<Type>::try_from(<ident>)`
/// (or `TryFrom::try_from(<ident>)`) and `<ident>` is the scrutinee of an
/// enclosing `match` arm whose pattern already matched a specific variant. Inside
/// such an arm the scrutinee is that variant, for which a variant-to-variant
/// `try_from` is conventionally total. This is a pragmatic exemption (the rule
/// cannot resolve the `TryFrom` impl to prove totality), not a soundness claim.
///
/// `call` is the `<receiver>.unwrap()` call_expression; `field_expr` is its
/// `<receiver>.unwrap` field_expression.
fn is_variant_discriminated_try_from(
    call: tree_sitter::Node,
    field_expr: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let Some(arg_ident) = try_from_argument_identifier(field_expr, source) else {
        return false;
    };
    // Walk up to each enclosing match arm; an arm whose match scrutinee is the
    // same identifier and whose pattern is a specific variant proves totality.
    let mut cur = call;
    while let Some(parent) = cur.parent() {
        // Stop at the function boundary — a match further out is unrelated.
        if matches!(
            cur.kind(),
            "function_item" | "closure_expression" | "source_file"
        ) {
            return false;
        }
        if parent.kind() == "match_arm"
            && arm_discriminates_scrutinee(parent, arg_ident, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// If `field_expr`'s receiver is a `<Type>::try_from(<ident>)` call (the function
/// is a `scoped_identifier` whose final segment is `try_from`) with a single
/// plain-identifier argument, return that argument's text. `None` otherwise.
fn try_from_argument_identifier<'a>(
    field_expr: tree_sitter::Node,
    source: &'a [u8],
) -> Option<&'a str> {
    let receiver = field_expr.child_by_field_name("value")?;
    if receiver.kind() != "call_expression" {
        return None;
    }
    let func = receiver.child_by_field_name("function")?;
    if func.kind() != "scoped_identifier" {
        return None;
    }
    if func
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        != Some("try_from")
    {
        return None;
    }
    let args = receiver.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut named = args.named_children(&mut cursor);
    let arg = named.next()?;
    if named.next().is_some() {
        return None; // try_from takes exactly one argument
    }
    if arg.kind() != "identifier" {
        return None;
    }
    arg.utf8_text(source).ok()
}

/// True when `arm` is a `match_arm` whose enclosing `match` scrutinee is the
/// identifier `scrutinee` and whose pattern matches a *specific* variant — i.e.
/// not a wildcard `_` and not a plain binding identifier (both of which match any
/// value and so provide no discrimination).
fn arm_discriminates_scrutinee(
    arm: tree_sitter::Node,
    scrutinee: &str,
    source: &[u8],
) -> bool {
    // arm -> match_block -> match_expression; the scrutinee sits in `value`.
    let Some(match_block) = arm.parent() else {
        return false;
    };
    let Some(match_expr) = match_block.parent() else {
        return false;
    };
    if match_expr.kind() != "match_expression" {
        return false;
    }
    let Some(value) = match_expr.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "identifier" || value.utf8_text(source).ok() != Some(scrutinee) {
        return false;
    }
    let Some(pattern) = arm.child_by_field_name("pattern") else {
        return false;
    };
    pattern_discriminates(pattern, source)
}

/// True when an arm `pattern` matches a specific variant rather than every value.
/// `_` (wildcard) and a plain binding identifier match anything and so do not
/// discriminate; a tuple-struct/struct/path/reference variant pattern does.
fn pattern_discriminates(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    // Unwrap the `match_pattern` wrapper (seq(_pattern, optional("if" guard))).
    // `_` surfaces as an unnamed token, so the wrapper has no named child.
    let inner = if pattern.kind() == "match_pattern" {
        match pattern.named_child(0) {
            Some(n) => n,
            None => return false, // bare `_`
        }
    } else {
        pattern
    };
    !matches!(inner.kind(), "wildcard_pattern" | "identifier")
}

/// True when the `.unwrap()`/`.expect()` receiver is `<recv>.downcast::<T>()`
/// (or `downcast_ref`/`downcast_mut`) and the call sits in the consequence of an
/// enclosing `if <recv>.is::<T>()` whose receiver and type argument both match.
/// `Any::downcast` returns `Ok`/`Some` exactly when `is::<T>()` is true, so the
/// guard makes the unwrap provably infallible. The match is conservative: it
/// requires identical receiver text AND identical type-argument text, so a
/// mismatched type (`is::<A>()` then `downcast::<B>()`) or a different receiver
/// is not exempted.
///
/// `call` is the `<receiver>.unwrap()` call_expression; `field_expr` is its
/// `<receiver>.unwrap` field_expression.
fn is_guarded_downcast_unwrap(
    call: tree_sitter::Node,
    field_expr: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let Some((receiver_text, type_text)) = downcast_receiver_and_type(field_expr, source) else {
        return false;
    };
    // Walk up to each enclosing `if`; an `if <receiver>.is::<T>()` guard whose
    // consequence contains this call proves the downcast cannot fail.
    let mut cur = call;
    while let Some(parent) = cur.parent() {
        // Stop at the function boundary — an `if` further out is unrelated.
        if matches!(
            cur.kind(),
            "function_item" | "closure_expression" | "source_file"
        ) {
            return false;
        }
        if parent.kind() == "if_expression"
            && parent
                .child_by_field_name("consequence")
                .is_some_and(|c| c.id() == cur.id())
            && let Some(condition) = parent.child_by_field_name("condition")
            && condition_has_is_guard(condition, receiver_text, type_text, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// If `field_expr`'s receiver is a `<recv>.downcast::<T>()` /
/// `.downcast_ref::<T>()` / `.downcast_mut::<T>()` call, return
/// `(receiver_text, type_argument_text)`. `None` otherwise.
fn downcast_receiver_and_type<'a>(
    field_expr: tree_sitter::Node,
    source: &'a [u8],
) -> Option<(&'a str, &'a str)> {
    let receiver = field_expr.child_by_field_name("value")?;
    if receiver.kind() != "call_expression" {
        return None;
    }
    let generic = receiver.child_by_field_name("function")?;
    if generic.kind() != "generic_function" {
        return None;
    }
    let func = generic.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let method = func.child_by_field_name("field")?.utf8_text(source).ok()?;
    if !matches!(method, "downcast" | "downcast_ref" | "downcast_mut") {
        return None;
    }
    let recv_text = func.child_by_field_name("value")?.utf8_text(source).ok()?;
    let type_text = sole_type_argument_text(generic, source)?;
    Some((recv_text, type_text))
}

/// True when `condition` contains a `<receiver>.is::<type>()` call whose receiver
/// text and sole type argument both equal the given downcast receiver and type.
/// Descends through `&&` chains and parenthesized expressions so the guard may be
/// one conjunct of a larger boolean condition.
fn condition_has_is_guard(
    condition: tree_sitter::Node,
    receiver_text: &str,
    type_text: &str,
    source: &[u8],
) -> bool {
    match condition.kind() {
        "parenthesized_expression" => condition
            .named_child(0)
            .is_some_and(|inner| condition_has_is_guard(inner, receiver_text, type_text, source)),
        "binary_expression" => {
            // Only `&&` distributes the guarantee to the consequence; `||` does not.
            let is_and = condition
                .child_by_field_name("operator")
                .and_then(|op| op.utf8_text(source).ok())
                == Some("&&");
            if !is_and {
                return false;
            }
            let left = condition.child_by_field_name("left");
            let right = condition.child_by_field_name("right");
            left.is_some_and(|n| condition_has_is_guard(n, receiver_text, type_text, source))
                || right
                    .is_some_and(|n| condition_has_is_guard(n, receiver_text, type_text, source))
        }
        "call_expression" => is_matching_is_call(condition, receiver_text, type_text, source),
        _ => false,
    }
}

/// True when `call` is `<receiver_text>.is::<type_text>()` — a `generic_function`
/// whose method is `is`, whose receiver text matches, and whose sole type
/// argument matches.
fn is_matching_is_call(
    call: tree_sitter::Node,
    receiver_text: &str,
    type_text: &str,
    source: &[u8],
) -> bool {
    let Some(generic) = call.child_by_field_name("function") else {
        return false;
    };
    if generic.kind() != "generic_function" {
        return false;
    }
    let Some(func) = generic.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    if func
        .child_by_field_name("field")
        .and_then(|n| n.utf8_text(source).ok())
        != Some("is")
    {
        return false;
    }
    if func
        .child_by_field_name("value")
        .and_then(|n| n.utf8_text(source).ok())
        != Some(receiver_text)
    {
        return false;
    }
    sole_type_argument_text(generic, source) == Some(type_text)
}

/// The single type argument's text of a `generic_function` `<f>::<T>`, or `None`
/// when there is not exactly one type argument.
fn sole_type_argument_text<'a>(
    generic: tree_sitter::Node,
    source: &'a [u8],
) -> Option<&'a str> {
    let args = generic.child_by_field_name("type_arguments")?;
    let mut cursor = args.walk();
    let mut named = args.named_children(&mut cursor);
    let first = named.next()?;
    if named.next().is_some() {
        return None;
    }
    first.utf8_text(source).ok()
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
    fn flags_unwrap_in_from_impl() {
        let source = "impl From<&str> for u32 { fn from(s: &str) -> Self { s.parse().unwrap() } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_in_from_impl() {
        let source = r#"impl From<String> for Url {
            fn from(s: String) -> Self { Url::parse(&s).expect("bad url") }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unwrap_in_try_from_impl() {
        let source = r#"impl TryFrom<&str> for u32 {
            type Error = ParseIntError;
            fn try_from(s: &str) -> Result<Self, Self::Error> { Ok(s.parse().unwrap()) }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_clean_from_impl() {
        let source = "impl From<u32> for u64 { fn from(x: u32) -> Self { x as u64 } }";
        assert!(run_on(source).is_empty());
    }

    /// Closes #3228: `FromRequest`/`FromRequestParts` are axum extractor traits
    /// returning `Result` with an associated `Rejection` — explicitly fallible,
    /// unrelated to `std::convert::From`. Their name merely begins with `From`,
    /// so the old `starts_with("From")` predicate flagged them. They must not be.
    #[test]
    fn allows_unwrap_in_from_request_impl() {
        let source = r#"impl<S> FromRequest<S> for X {
            async fn from_request(mut req: Request, state: &S) -> Result<Self, Self::Rejection> {
                let v = req.extract_parts().await.unwrap();
                Ok(Self { v })
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_in_from_request_parts_impl() {
        let source = r#"impl FromRequestParts<S> for X {
            async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
                let v = parts.extract().await.unwrap();
                Ok(Self { v })
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_in_from_str_impl() {
        let source = r#"impl FromStr for X {
            type Err = ParseIntError;
            fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(X(s.parse().unwrap())) }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_in_from_iterator_impl() {
        let source = r#"impl<T> FromIterator<T> for X {
            fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
                X(iter.into_iter().next().unwrap())
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    /// A qualified `core::convert::From<...>` is still the real `From` trait and
    /// must stay flagged via the `::From<` branch of the predicate.
    #[test]
    fn flags_unwrap_in_qualified_from_impl() {
        let source = r#"impl core::convert::From<String> for X {
            fn from(s: String) -> Self { X(s.parse().unwrap()) }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// Closes #5171: a `From` impl living under a `tests/` directory is a test
    /// helper where a panicking conversion is acceptable (the test fails loudly).
    /// `skip_in_test_dir` makes the engine skip the rule there entirely. A
    /// production `From` impl with the same `.unwrap()` is still flagged.
    #[test]
    fn skips_from_impl_in_tests_dir() {
        let source = r#"impl<F: Fn(&str) -> String> From<OutputFormatter<F>> for Stdio {
            fn from(output: OutputFormatter<F>) -> Stdio {
                let (read_end, write_end) = os_pipe::pipe().unwrap();
                Stdio::from(write_end)
            }
        }"#;
        let in_tests =
            crate::rules::test_helpers::run_rule_gated(&Check, source, "cargo-insta/tests/functional/main.rs");
        assert!(
            in_tests.is_empty(),
            "a From impl in a tests/ directory is a test helper; unwrap is acceptable"
        );
        let in_src =
            crate::rules::test_helpers::run_rule_gated(&Check, source, "src/conversion.rs");
        assert_eq!(
            in_src.len(),
            1,
            "a production From impl with .unwrap() is still flagged"
        );
    }

    /// Closes #3799: a `.unwrap()` on a statement gated by
    /// `#[cfg(debug_assertions)]` compiles out entirely in release builds, so
    /// the conversion has no runtime fallible path — the idiomatic equivalent
    /// of `debug_assert!`. It must not be flagged.
    #[test]
    fn allows_unwrap_gated_by_cfg_debug_assertions() {
        let source = "impl From<Column> for BlockEntry {\n    fn from(col: Column) -> Self {\n        #[cfg(debug_assertions)]\n        col.check_valid().unwrap();\n        BlockEntry::Column(col)\n    }\n}";
        assert!(
            run_on(source).is_empty(),
            "a #[cfg(debug_assertions)]-gated unwrap is a debug-only check, not a release failure path"
        );
    }

    /// A `#[cfg(feature = "x")]` gate leaves the statement in release builds —
    /// it is a real runtime path, so the unwrap must still flag. The exemption
    /// is `debug_assertions`-specific.
    #[test]
    fn flags_unwrap_gated_by_cfg_feature() {
        let source = "impl From<&str> for u32 {\n    fn from(s: &str) -> Self {\n        #[cfg(feature = \"x\")]\n        return s.parse().unwrap();\n        0\n    }\n}";
        assert_eq!(
            run_on(source).len(),
            1,
            "a #[cfg(feature = \"x\")]-gated unwrap is a real release path and must still flag"
        );
    }

    /// Closes #4409: a `.expect("invariant broken: …")` documents a condition
    /// guaranteed by a validated newtype, so the `try_from` can never fail. The
    /// message asserts an infallible invariant, not a runtime failure path.
    #[test]
    fn allows_expect_documenting_invariant() {
        let source = r#"impl From<NonNegativeI64> for u64 {
            fn from(x: NonNegativeI64) -> u64 {
                u64::try_from(x.0).expect("invariant broken: NonNegativeI64 should contain a non-negative i64 value")
            }
        }"#;
        assert!(
            run_on(source).is_empty(),
            "an `.expect()` documenting an infallible invariant is not a runtime failure path"
        );
    }

    /// An `.expect("unreachable: …")` also documents a guaranteed condition and
    /// must not be flagged.
    #[test]
    fn allows_expect_documenting_unreachable() {
        let source = r#"impl From<A> for B {
            fn from(a: A) -> B { build(a).expect("unreachable: validated on construction") }
        }"#;
        assert!(run_on(source).is_empty());
    }

    /// A bare `.unwrap()` has no message documenting an invariant, so the
    /// exemption must not catch it — it stays flagged.
    #[test]
    fn flags_bare_unwrap_in_from_impl() {
        let source = "impl From<A> for B { fn from(a: A) -> B { something(a).unwrap() } }";
        assert_eq!(run_on(source).len(), 1);
    }

    /// An `.expect()` whose message does not mention an invariant is a real
    /// failure path — the exemption requires the invariant/unreachable keyword,
    /// so this must still flag.
    #[test]
    fn flags_expect_with_non_invariant_message() {
        let source =
            r#"impl From<A> for B { fn from(a: A) -> B { parse(a).expect("failed to parse input") } }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// Closes #4420: `NonZeroI64::new(1).unwrap()` is provably infallible —
    /// `NonZero*::new(n)` is `None` only for `n == 0`, and `1` is a non-zero
    /// literal — so the unwrap cannot panic and must not be flagged.
    #[test]
    fn allows_unwrap_on_nonzero_new_literal() {
        let source =
            "impl From<A> for B { fn from(a: A) -> B { B::E(NonZeroI64::new(1).unwrap()) } }";
        assert!(
            run_on(source).is_empty(),
            "NonZeroI64::new(1).unwrap() is provably infallible"
        );
    }

    /// A larger non-zero literal is equally infallible.
    #[test]
    fn allows_unwrap_on_nonzero_new_large_literal() {
        let source =
            "impl From<A> for B { fn from(a: A) -> B { B::E(NonZeroU8::new(255).unwrap()) } }";
        assert!(run_on(source).is_empty());
    }

    /// A fully-qualified `std::num::NonZeroUsize::new(8)` path resolves to the
    /// same infallible shape and must not be flagged.
    #[test]
    fn allows_unwrap_on_fully_qualified_nonzero_new_literal() {
        let source = "impl From<A> for B { fn from(a: A) -> B { B::E(std::num::NonZeroUsize::new(8).unwrap()) } }";
        assert!(run_on(source).is_empty());
    }

    /// A zero literal makes `NonZero*::new(0)` return `None`, so the unwrap
    /// genuinely panics — it must still flag.
    #[test]
    fn flags_unwrap_on_nonzero_new_zero_literal() {
        let source =
            "impl From<A> for B { fn from(a: A) -> B { B::E(NonZeroI64::new(0).unwrap()) } }";
        assert_eq!(run_on(source).len(), 1);
    }

    /// A non-literal argument is not provably non-zero, so the unwrap may
    /// panic — it must still flag.
    #[test]
    fn flags_unwrap_on_nonzero_new_variable() {
        let source =
            "impl From<A> for B { fn from(a: A) -> B { B::E(NonZeroI64::new(n).unwrap()) } }";
        assert_eq!(run_on(source).len(), 1);
    }

    /// Closes #4681: each `<Type>::try_from(color).unwrap()` sits in a match arm
    /// that already discriminated `color` to a specific variant
    /// (`Color::Rgb(..)`, `Color::Indexed(..)`), so the `try_from` is total and
    /// cannot fail. Those two unwraps must not be flagged. The trailing `_` arm
    /// does not discriminate to a single variant, so its unwrap still flags.
    #[test]
    fn allows_variant_discriminated_try_from_unwrap() {
        let source = r#"impl From<Color> for anstyle::Color {
            fn from(color: Color) -> Self {
                match color {
                    Color::Reset => panic!("Color::Reset has no equivalent in anstyle"),
                    Color::Rgb(_, _, _) => Self::Rgb(RgbColor::try_from(color).unwrap()),
                    Color::Indexed(_) => Self::Ansi256(Ansi256Color::try_from(color).unwrap()),
                    _ => Self::Ansi(AnsiColor::try_from(color).unwrap()),
                }
            }
        }"#;
        // Only the `_` arm's unwrap remains flagged.
        assert_eq!(
            run_on(source).len(),
            1,
            "variant-discriminated try_from unwraps are infallible; only the `_` arm flags"
        );
    }

    /// A `try_from(x).unwrap()` with no enclosing match has no discrimination
    /// invariant, so it is a real fallible path and must still flag.
    #[test]
    fn flags_try_from_unwrap_without_match() {
        let source =
            "impl From<A> for B { fn from(a: A) -> B { B(RgbColor::try_from(a).unwrap()) } }";
        assert_eq!(run_on(source).len(), 1);
    }

    /// A `_` wildcard arm does not constrain the scrutinee to a specific variant,
    /// so a `try_from` inside it is not provably total — it must still flag.
    #[test]
    fn flags_try_from_unwrap_in_wildcard_arm() {
        let source = r#"impl From<Color> for X {
            fn from(color: Color) -> Self {
                match color {
                    _ => X(RgbColor::try_from(color).unwrap()),
                }
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// A plain binding identifier arm (`other => ...`) binds any value without
    /// discriminating a variant, so the `try_from` is not provably total and the
    /// unwrap must still flag.
    #[test]
    fn flags_try_from_unwrap_in_binding_arm() {
        let source = r#"impl From<Color> for X {
            fn from(color: Color) -> Self {
                match color {
                    other => X(RgbColor::try_from(color).unwrap()),
                }
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// The exemption requires the matched identifier to BE the `try_from`
    /// argument. A variant arm matching `color` but unwrapping a `try_from(other)`
    /// over a different value provides no invariant — it must still flag.
    #[test]
    fn flags_try_from_unwrap_on_unrelated_value() {
        let source = r#"impl From<Color> for X {
            fn from(color: Color) -> Self {
                match color {
                    Color::Rgb(_, _, _) => X(RgbColor::try_from(other).unwrap()),
                    _ => X::default(),
                }
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// An or-pattern of specific variants (`A | B`) still discriminates away from
    /// `_` and plain bindings, so the exemption applies.
    #[test]
    fn allows_try_from_unwrap_in_or_pattern_arm() {
        let source = r#"impl From<Color> for X {
            fn from(color: Color) -> Self {
                match color {
                    Color::Rgb(_, _, _) | Color::Indexed(_) => X(C::try_from(color).unwrap()),
                    _ => X::default(),
                }
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    /// Closes #4759: `bitset.serialize(&mut buffer)` where `buffer` is a local
    /// `Vec` is an `io::Write` into a heap buffer, whose impl never returns
    /// `Err`. The `.expect()` is a documentation-only assertion that cannot
    /// panic at runtime, so it must not be flagged.
    #[test]
    fn allows_expect_on_serialize_into_local_vec() {
        let source = r#"impl<'a> From<&'a BitSet> for ReadOnlyBitSet {
            fn from(bitset: &'a BitSet) -> ReadOnlyBitSet {
                let mut buffer = Vec::with_capacity(bitset.tinysets.len() * 8 + 4);
                bitset
                    .serialize(&mut buffer)
                    .expect("serializing into a buffer should never fail");
                ReadOnlyBitSet::open(OwnedBytes::new(buffer))
            }
        }"#;
        assert!(
            run_on(source).is_empty(),
            "serializing into an in-memory Vec<u8> buffer is infallible"
        );
    }

    /// `buf.write_all(b"…")` where `buf` is a local `Vec` is the direct
    /// `io::Write`-into-buffer form and is equally infallible.
    #[test]
    fn allows_unwrap_on_write_all_into_local_vec() {
        let source = r#"impl From<A> for B {
            fn from(a: A) -> B {
                let mut buf = Vec::new();
                buf.write_all(b"hello").unwrap();
                B(buf)
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    /// `write!(&mut s, …)` into a local `String` uses `fmt::Write`, which never
    /// returns `Err` for an in-memory `String` — it must not be flagged.
    #[test]
    fn allows_unwrap_on_write_macro_into_local_string() {
        let source = r#"impl From<u32> for B {
            fn from(n: u32) -> B {
                let mut s = String::new();
                write!(&mut s, "{}", n).unwrap();
                B(s)
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    /// A genuinely fallible write — into a `File`, not an in-memory buffer —
    /// can return `Err` (disk full, broken pipe), so the unwrap is a real
    /// failure path and must still flag.
    #[test]
    fn flags_unwrap_on_write_into_file() {
        let source = r#"impl From<A> for B {
            fn from(a: A) -> B {
                let mut file = File::create("out.bin").unwrap();
                file.write_all(b"hello").unwrap();
                B
            }
        }"#;
        // Both the `File::create(..).unwrap()` and the `file.write_all(..).unwrap()`
        // are real fallible paths (`file` is not a Vec/String buffer).
        assert_eq!(run_on(source).len(), 2);
    }

    /// The buffer exemption requires the writer to be a local `Vec`/`String`. A
    /// `serialize(&mut writer)` where `writer` is a function parameter of unknown
    /// type is not provably infallible, so the unwrap must still flag.
    #[test]
    fn flags_serialize_into_unknown_writer() {
        let source = r#"impl From<A> for B {
            fn from(a: A) -> B {
                a.serialize(&mut writer).expect("write failed");
                B
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// Characterization of the deliberate limit: a `TryFrom` impl could return
    /// `Err` even for a discriminated variant, but the rule has no type
    /// resolution and cannot tell. This case is intentionally exempted (lint
    /// false-negative) to kill the false-positive on the total idiom — locking it
    /// in so a later change does not silently reintroduce the FP.
    #[test]
    fn allows_variant_discriminated_try_from_even_if_impl_could_fail() {
        let source = r#"impl From<Color> for X {
            fn from(color: Color) -> Self {
                match color {
                    Color::Rgb(_, _, _) => X(Ansi256::try_from(color).unwrap()),
                    _ => X::default(),
                }
            }
        }"#;
        assert!(
            run_on(source).is_empty(),
            "variant-discriminated try_from is exempted by design; the rule cannot prove totality"
        );
    }

    /// Closes #5029: a `<recv>.downcast::<T>().unwrap()` inside an
    /// `if <recv>.is::<T>()` branch is provably infallible — `Any::downcast`
    /// succeeds whenever `is::<T>()` is true (same receiver, same type) — so it
    /// must not be flagged.
    #[test]
    fn allows_is_guarded_downcast_unwrap() {
        let source = r#"impl From<Box<dyn Any>> for Error {
            fn from(value: Box<dyn Any>) -> Self {
                if value.is::<Error>() {
                    return Self::Wrapped(value.downcast::<Error>().unwrap());
                }
                Self::Other
            }
        }"#;
        assert!(
            run_on(source).is_empty(),
            "an is::<T>()-guarded downcast::<T>().unwrap() cannot fail"
        );
    }

    /// `downcast_ref::<T>()` / `downcast_mut::<T>()` are equally guarded by a
    /// matching `is::<T>()` check and must not be flagged either.
    #[test]
    fn allows_is_guarded_downcast_ref_unwrap() {
        let source = r#"impl From<&dyn Any> for B {
            fn from(value: &dyn Any) -> Self {
                if value.is::<Foo>() {
                    return B(value.downcast_ref::<Foo>().unwrap().clone());
                }
                B::default()
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    /// The guard may be one conjunct of a larger `&&` condition; the downcast is
    /// still dominated by the matching `is::<T>()` check.
    #[test]
    fn allows_is_guarded_downcast_in_and_condition() {
        let source = r#"impl From<Box<dyn Any>> for Error {
            fn from(value: Box<dyn Any>) -> Self {
                if ready && value.is::<Error>() {
                    return Self::Wrapped(value.downcast::<Error>().unwrap());
                }
                Self::Other
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    /// A bare `.unwrap()` with no enclosing `is::<T>()` guard is a real fallible
    /// path and must still flag — the exemption is guard-specific.
    #[test]
    fn flags_unguarded_downcast_unwrap() {
        let source = r#"impl From<Box<dyn Any>> for Error {
            fn from(value: Box<dyn Any>) -> Self {
                Self::Wrapped(value.downcast::<Error>().unwrap())
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// A mismatched type — `is::<A>()` guarding a `downcast::<B>()` — does NOT
    /// prove the downcast succeeds, so it must still flag.
    #[test]
    fn flags_is_guarded_downcast_with_mismatched_type() {
        let source = r#"impl From<Box<dyn Any>> for Error {
            fn from(value: Box<dyn Any>) -> Self {
                if value.is::<A>() {
                    return Self::Wrapped(value.downcast::<B>().unwrap());
                }
                Self::Other
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// A mismatched receiver — `other.is::<T>()` guarding `value.downcast::<T>()`
    /// — proves nothing about `value`, so it must still flag.
    #[test]
    fn flags_is_guarded_downcast_with_mismatched_receiver() {
        let source = r#"impl From<Box<dyn Any>> for Error {
            fn from(value: Box<dyn Any>) -> Self {
                if other.is::<Error>() {
                    return Self::Wrapped(value.downcast::<Error>().unwrap());
                }
                Self::Other
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// A guard reached only via `||` does not dominate the consequence (the
    /// branch can execute when the `is::<T>()` conjunct is false), so the unwrap
    /// must still flag.
    #[test]
    fn flags_is_guarded_downcast_in_or_condition() {
        let source = r#"impl From<Box<dyn Any>> for Error {
            fn from(value: Box<dyn Any>) -> Self {
                if forced || value.is::<Error>() {
                    return Self::Wrapped(value.downcast::<Error>().unwrap());
                }
                Self::Other
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// The exemption requires the unwrap to be in the `if`'s consequence, not its
    /// `else` branch — there `is::<T>()` is false, so the downcast can fail.
    #[test]
    fn flags_downcast_unwrap_in_else_branch() {
        let source = r#"impl From<Box<dyn Any>> for Error {
            fn from(value: Box<dyn Any>) -> Self {
                if value.is::<Other>() {
                    Self::Other
                } else {
                    Self::Wrapped(value.downcast::<Error>().unwrap())
                }
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }
}
