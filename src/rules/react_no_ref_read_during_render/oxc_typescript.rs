//! react-no-ref-read-during-render OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub struct Check;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn starts_with_use_hook(name: &str) -> bool {
    name.starts_with("use") && name.chars().nth(3).is_some_and(|c| c.is_ascii_uppercase())
}

/// Collect ref binding names from `const x = useRef(...)` declarations in a
/// function body. We walk the semantic nodes whose parent chain includes the
/// body node.
fn collect_ref_bindings<'a>(
    body_span: oxc_span::Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> FxHashSet<String> {
    let mut refs = FxHashSet::default();
    for node in semantic.nodes().iter() {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            continue;
        };
        // Must be inside the body
        if decl.span.start < body_span.start || decl.span.end > body_span.end {
            continue;
        }
        let Some(init) = &decl.init else { continue };
        let oxc_ast::ast::Expression::CallExpression(call) = init else {
            continue;
        };
        let callee_text = &source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "useRef" && !callee_text.ends_with(".useRef") {
            continue;
        }
        let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &decl.id else {
            continue;
        };
        refs.insert(ident.name.to_string());
    }
    refs
}

/// True if a `useRef(...)` argument is a safe-default initial value: a literal
/// (`0`, `''`, `false`, `null`), an empty array/object, or a negated/unary
/// literal (`-1`). `useRef(0)` is safe to read during render before the
/// post-mount effect runs; `useRef()` (undefined) and `useRef(someExpr)` are not
/// covered by the post-mount exemption.
fn is_safe_default_init(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::ArrayExpression(_)
        | Expression::ObjectExpression(_) => true,
        Expression::UnaryExpression(unary) => is_safe_default_init(&unary.argument),
        _ => false,
    }
}

/// Collect ref binding names whose `useRef(...)` initializer is a safe default
/// (see `is_safe_default_init`).
fn collect_safe_default_refs<'a>(
    body_span: oxc_span::Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> FxHashSet<String> {
    let mut refs = FxHashSet::default();
    for node in semantic.nodes().iter() {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            continue;
        };
        if decl.span.start < body_span.start || decl.span.end > body_span.end {
            continue;
        }
        let Some(oxc_ast::ast::Expression::CallExpression(call)) = &decl.init else {
            continue;
        };
        let callee_text =
            &source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "useRef" && !callee_text.ends_with(".useRef") {
            continue;
        }
        let Some(arg) = call.arguments.first().and_then(|a| a.as_expression()) else {
            continue;
        };
        if !is_safe_default_init(arg) {
            continue;
        }
        let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &decl.id else {
            continue;
        };
        refs.insert(ident.name.to_string());
    }
    refs
}

/// Collect the names of refs that are written ONLY inside a post-mount effect
/// (`useLayoutEffect`/`useEffect` callback with an empty dep array `[]`) and
/// never during render, and whose `useRef` init is a safe default literal.
///
/// Such a ref is never mutated during render, so reading `ref.current` during
/// render cannot tear — this is the documented post-mount-measurement pattern
/// (e.g. capturing `element.offsetTop` once after mount to feed a layout config
/// input). The init being a safe default guarantees the first-render read is
/// well-defined before the effect runs.
fn collect_post_mount_effect_only_refs<'a>(
    body_span: oxc_span::Span,
    refs: &FxHashSet<String>,
    safe_default_refs: &FxHashSet<String>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> FxHashSet<String> {
    // Spans of post-mount-effect callbacks (`useLayoutEffect`/`useEffect`
    // called with an empty-array 2nd arg) inside this component body.
    let mut effect_callback_spans: Vec<oxc_span::Span> = Vec::new();
    for node in semantic.nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        if call.span.start < body_span.start || call.span.end > body_span.end {
            continue;
        }
        let callee_text =
            &source[call.callee.span().start as usize..call.callee.span().end as usize];
        let is_effect = callee_text == "useEffect"
            || callee_text == "useLayoutEffect"
            || callee_text.ends_with(".useEffect")
            || callee_text.ends_with(".useLayoutEffect");
        if !is_effect || call.arguments.len() != 2 {
            continue;
        }
        let Some(oxc_ast::ast::Expression::ArrayExpression(deps)) =
            call.arguments[1].as_expression()
        else {
            continue;
        };
        if !deps.elements.is_empty() {
            continue;
        }
        let Some(callback) = call.arguments[0].as_expression() else {
            continue;
        };
        let cb_span = match callback {
            oxc_ast::ast::Expression::ArrowFunctionExpression(arrow) => arrow.body.span,
            oxc_ast::ast::Expression::FunctionExpression(func) => {
                let Some(b) = &func.body else { continue };
                b.span
            }
            _ => continue,
        };
        effect_callback_spans.push(cb_span);
    }

    let span_inside_effect = |span: oxc_span::Span| {
        effect_callback_spans
            .iter()
            .any(|cb| span.start >= cb.start && span.end <= cb.end)
    };

    // Classify every `ref.current` write target (assignment LHS or update arg):
    // written in render (disqualifies) vs written in a post-mount effect.
    let mut written_in_render: FxHashSet<String> = FxHashSet::default();
    let mut written_in_effect: FxHashSet<String> = FxHashSet::default();

    for node in semantic.nodes().iter() {
        let write = match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) =
                    &assign.left
                else {
                    continue;
                };
                Some(member.as_ref())
            }
            AstKind::UpdateExpression(update) => {
                let oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(member) =
                    &update.argument
                else {
                    continue;
                };
                Some(member.as_ref())
            }
            _ => continue,
        };
        let Some(member) = write else { continue };
        if member.property.name.as_str() != "current" {
            continue;
        }
        if member.span.start < body_span.start || member.span.end > body_span.end {
            continue;
        }
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            continue;
        };
        let name = obj.name.as_str();
        if !refs.contains(name) {
            continue;
        }
        if span_inside_effect(member.span) {
            written_in_effect.insert(name.to_string());
        } else {
            written_in_render.insert(name.to_string());
        }
    }

    written_in_effect
        .into_iter()
        .filter(|name| !written_in_render.contains(name) && safe_default_refs.contains(name))
        .collect()
}

/// True if `expr` is the member expression `<ref_name>.current`.
fn is_ref_current_read(expr: &oxc_ast::ast::Expression, ref_name: &str) -> bool {
    let oxc_ast::ast::Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    if member.property.name.as_str() != "current" {
        return false;
    }
    let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == ref_name
}

/// True if `expr` is a nullish literal: `null`, `undefined`, or `void 0`.
fn is_nullish_literal(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::{Expression, UnaryOperator};
    match expr {
        Expression::NullLiteral(_) => true,
        Expression::Identifier(ident) => ident.name.as_str() == "undefined",
        Expression::UnaryExpression(unary) => unary.operator == UnaryOperator::Void,
        _ => false,
    }
}

/// True if `expr` is a nullish guard on `<ref_name>.current` — i.e. it tests
/// that the ref is still unset before the lazy write runs. Recognizes:
/// - `!ref.current` (logical-not),
/// - `ref.current === null`/`== null`/`=== undefined`/`=== void 0` (and the
///   mirrored `null === ref.current`),
/// - a `||` `LogicalExpression` where EVERY operand is itself a nullish guard
///   on the same ref (e.g. `!ref.current || ref.current === undefined`).
///
/// A `||` test gates a one-time init only when all disjuncts are nullish
/// self-guards: `A || B` runs the consequent when either is true, so a single
/// non-guard operand (`cond || !ref.current`) lets the write run on later
/// renders and would not gate the init.
///
/// Deliberately does NOT match the truthy test `if (ref.current)` — that is the
/// opposite condition (write when already set) and would not gate a one-time
/// init.
fn is_nullish_guard_on_ref(expr: &oxc_ast::ast::Expression, ref_name: &str) -> bool {
    use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator, UnaryOperator};
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            is_nullish_guard_on_ref(&paren.expression, ref_name)
        }
        Expression::UnaryExpression(unary) => {
            unary.operator == UnaryOperator::LogicalNot
                && is_ref_current_read(&unary.argument, ref_name)
        }
        Expression::BinaryExpression(bin) => {
            let is_eq = matches!(
                bin.operator,
                BinaryOperator::Equality | BinaryOperator::StrictEquality
            );
            if !is_eq {
                return false;
            }
            (is_ref_current_read(&bin.left, ref_name) && is_nullish_literal(&bin.right))
                || (is_ref_current_read(&bin.right, ref_name) && is_nullish_literal(&bin.left))
        }
        Expression::LogicalExpression(logical) => {
            // `A || B` runs the consequent when EITHER operand is true, so it
            // gates a one-time init only when EVERY operand is itself a nullish
            // self-guard on the same ref. A single non-guard disjunct (e.g.
            // `cond || !ref.current`) lets the write run on later renders →
            // not lazy-init, so the render read can still tear.
            logical.operator == LogicalOperator::Or
                && is_nullish_guard_on_ref(&logical.left, ref_name)
                && is_nullish_guard_on_ref(&logical.right, ref_name)
        }
        _ => false,
    }
}

/// True if `expr` is a "change detector" on some ref's `.current`: a `!==`/`!=`
/// inequality where one side is `<anyRef>.current` (e.g.
/// `prevProp.current !== props.value`), optionally `&&`-guarded by other
/// expressions (`props.value && prevProp.current !== props.value`).
///
/// A change detector is self-limiting: it fires only when the compared value
/// differs from the value recorded in the ref on a prior render, and the gated
/// consequent records the new value — so on a re-render with the same input it
/// is false and the write does not re-run. That makes the post-write reads
/// stable within a render. An arbitrary disjunct (a bare identifier, a call, an
/// equality test) is NOT self-limiting: it can be true every render, re-running
/// the write and tearing the read, so it does not qualify.
fn is_change_detector_on_some_ref(
    expr: &oxc_ast::ast::Expression,
    refs: &FxHashSet<String>,
) -> bool {
    use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator};
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            is_change_detector_on_some_ref(&paren.expression, refs)
        }
        // `guard && <detector>` (or `<detector> && guard`): the inequality must
        // still be present on one side.
        Expression::LogicalExpression(logical) if logical.operator == LogicalOperator::And => {
            is_change_detector_on_some_ref(&logical.left, refs)
                || is_change_detector_on_some_ref(&logical.right, refs)
        }
        Expression::BinaryExpression(bin) => {
            let is_neq = matches!(
                bin.operator,
                BinaryOperator::Inequality | BinaryOperator::StrictInequality
            );
            if !is_neq {
                return false;
            }
            ref_current_object_name(&bin.left, refs)
                || ref_current_object_name(&bin.right, refs)
        }
        _ => false,
    }
}

/// True if `expr` is `<name>.current` where `name` is one of `refs`.
fn ref_current_object_name(expr: &oxc_ast::ast::Expression, refs: &FxHashSet<String>) -> bool {
    let oxc_ast::ast::Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    if member.property.name.as_str() != "current" {
        return false;
    }
    let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
        return false;
    };
    refs.contains(obj.name.as_str())
}

/// True if `test` is a controlled-re-init guard: a `||` disjunction where, for
/// SOME ref R, one disjunct nullish-self-guards R (`!R.current`, `=== null`, …)
/// and EVERY OTHER disjunct is either another nullish self-guard on R or a
/// change detector on some ref's `.current` (see `is_change_detector_on_some_ref`).
///
/// This is the react-hook-form `useForm` shape:
/// `!_formControl.current || (props.formControl && _formControlProp.current !== props.formControl)`.
/// The nullish self-guard makes the first render run the init; every other
/// disjunct is a change detector that re-fires only on a genuine input change,
/// so the gated writes are one-time-per-distinct-input and the post-write reads
/// are stable within a render — no tearing. A disjunct that is neither (a bare
/// `cond`, a call, an equality) can run the write every render and defeats the
/// gate, so the whole `if` is rejected.
fn is_controlled_reinit_guard(test: &oxc_ast::ast::Expression, refs: &FxHashSet<String>) -> bool {
    use oxc_ast::ast::{Expression, LogicalOperator};
    let Expression::LogicalExpression(logical) = test else {
        return false;
    };
    if logical.operator != LogicalOperator::Or {
        return false;
    }
    let mut disjuncts: Vec<&Expression> = Vec::new();
    collect_or_disjuncts(test, &mut disjuncts);

    // Find the ref nullish-self-guarded by some disjunct, then require every
    // remaining disjunct to be a nullish guard on that same ref or a change
    // detector. Try each ref so the nullish-guard and the detectors need not
    // name the same ref (RHF guards `_formControl` but detects on
    // `_formControlProp`).
    refs.iter().any(|guarded| {
        let mut has_nullish_self_guard = false;
        for disjunct in &disjuncts {
            if is_nullish_guard_on_ref(disjunct, guarded) {
                has_nullish_self_guard = true;
            } else if !is_change_detector_on_some_ref(disjunct, refs) {
                return false;
            }
        }
        has_nullish_self_guard
    })
}

/// Flatten a left-associative `||` chain into its leaf disjuncts.
fn collect_or_disjuncts<'a>(
    expr: &'a oxc_ast::ast::Expression<'a>,
    out: &mut Vec<&'a oxc_ast::ast::Expression<'a>>,
) {
    use oxc_ast::ast::{Expression, LogicalOperator};
    match expr {
        Expression::LogicalExpression(logical) if logical.operator == LogicalOperator::Or => {
            collect_or_disjuncts(&logical.left, out);
            collect_or_disjuncts(&logical.right, out);
        }
        Expression::ParenthesizedExpression(paren) => collect_or_disjuncts(&paren.expression, out),
        _ => out.push(expr),
    }
}

/// Collect the names of refs that follow React's sanctioned lazy-init pattern:
/// every render-time write to `ref.current` sits inside the consequent of an
/// `if` whose test nullish-guards THAT SAME ref (`if (!ref.current) { ref.current = make(); }`).
///
/// React's docs ("Avoiding recreating the ref contents") allow reading such a
/// ref during render: the write runs only on the first render (the guard is
/// false thereafter), so every subsequent read returns the same stable object —
/// no tearing.
///
/// The classification requires (a) at least one write to `ref.current`, and
/// (b) that EVERY such write be nullish-self-guarded. The guard guarantees the
/// write runs at most once regardless of where it sits, so a guarded ref is
/// stable after init. A ref written unconditionally (`ref.current = compute()`)
/// has an unguarded write and is therefore NOT lazy-init — its read still
/// flags, guarding against the tearing false-negative.
fn collect_lazy_init_refs<'a>(
    body_span: oxc_span::Span,
    refs: &FxHashSet<String>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> FxHashSet<String> {
    // Consequent spans of `if`-statements whose test nullish-guards a given ref,
    // keyed by the guarded ref name.
    let mut guarded_consequents: Vec<(String, oxc_span::Span)> = Vec::new();
    for node in semantic.nodes().iter() {
        let AstKind::IfStatement(if_stmt) = node.kind() else {
            continue;
        };
        if if_stmt.span.start < body_span.start || if_stmt.span.end > body_span.end {
            continue;
        }
        // Only guards in the component's own render scope gate render-time
        // reads; a guard inside a nested closure says nothing about render.
        if is_inside_nested_function(node.id(), body_span, semantic) {
            continue;
        }
        for name in refs {
            if is_nullish_guard_on_ref(&if_stmt.test, name) {
                guarded_consequents.push((name.clone(), if_stmt.consequent.span()));
            }
        }
    }

    // Classify each write to `ref.current`: is it inside the consequent of an
    // `if` that nullish-guards THAT SAME ref?
    let mut has_any_write: FxHashSet<String> = FxHashSet::default();
    let mut has_unguarded_write: FxHashSet<String> = FxHashSet::default();

    for node in semantic.nodes().iter() {
        let member = match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left
                else {
                    continue;
                };
                member.as_ref()
            }
            AstKind::UpdateExpression(update) => {
                let oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(member) =
                    &update.argument
                else {
                    continue;
                };
                member.as_ref()
            }
            _ => continue,
        };
        if member.property.name.as_str() != "current" {
            continue;
        }
        if member.span.start < body_span.start || member.span.end > body_span.end {
            continue;
        }
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            continue;
        };
        let name = obj.name.as_str();
        if !refs.contains(name) {
            continue;
        }
        // Only render-scope writes bear on lazy-init; a write inside a nested
        // closure (handler/effect) runs outside render and is irrelevant here.
        if is_inside_nested_function(node.id(), body_span, semantic) {
            continue;
        }
        has_any_write.insert(name.to_string());
        let guarded = guarded_consequents.iter().any(|(guarded_name, span)| {
            guarded_name == name && member.span.start >= span.start && member.span.end <= span.end
        });
        if !guarded {
            has_unguarded_write.insert(name.to_string());
        }
    }

    has_any_write
        .into_iter()
        .filter(|name| !has_unguarded_write.contains(name))
        .collect()
}

/// A gating `if` for a controlled-re-init ref: the full statement span (reads
/// after `stmt.end` are stable) and the test span (the change-detector read of
/// the ref inside the guard is also stable — it reads the value recorded on a
/// prior render before deciding whether to re-init).
#[derive(Clone, Copy)]
struct ReinitGate {
    stmt: oxc_span::Span,
    test: oxc_span::Span,
}

/// Collect refs that follow the controlled prop-change re-init pattern, keyed by
/// ref name → the gating `if` blocks. A ref qualifies when it has at least one
/// render-scope write to `ref.current` and EVERY such write sits inside the
/// consequent of an `if` whose test is a controlled re-init guard
/// (`is_controlled_reinit_guard`). All refs written solely inside the gated
/// consequents qualify — both the nullish-guarded ref (`_formControl`) and any
/// sibling tracker ref written alongside it (`_formControlProp`), which the
/// change detector reads.
///
/// The returned gates let the caller exempt only reads that are provably stable:
/// a read inside the gate's test (the change-detector read) or strictly after
/// the gate's `if` block. A read positioned BEFORE the gate could observe the
/// pre-re-init value while a later read observes the post-re-init value —
/// tearing within the render — so it is not exempted here and still flags.
fn collect_controlled_reinit_refs<'a>(
    body_span: oxc_span::Span,
    refs: &FxHashSet<String>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> FxHashMap<String, Vec<ReinitGate>> {
    use rustc_hash::FxHashMap;

    // `if`-statements in render scope whose test is a controlled re-init guard,
    // paired with their consequent span.
    let mut gated: Vec<(ReinitGate, oxc_span::Span)> = Vec::new();
    for node in semantic.nodes().iter() {
        let AstKind::IfStatement(if_stmt) = node.kind() else {
            continue;
        };
        if if_stmt.span.start < body_span.start || if_stmt.span.end > body_span.end {
            continue;
        }
        if is_inside_nested_function(node.id(), body_span, semantic) {
            continue;
        }
        if is_controlled_reinit_guard(&if_stmt.test, refs) {
            gated.push((
                ReinitGate {
                    stmt: if_stmt.span,
                    test: if_stmt.test.span(),
                },
                if_stmt.consequent.span(),
            ));
        }
    }
    if gated.is_empty() {
        return FxHashMap::default();
    }

    // Classify each render-scope write to `ref.current`, recording for each ref
    // the gate(s) whose consequent contains that write. A ref with any write
    // outside every gated consequent is mutated unconditionally and is not
    // exempt; each exempt ref is keyed ONLY to the gates that actually re-init
    // it, so a read of one ref is never exempted by an unrelated ref's gate.
    let mut writes_into_gates: FxHashMap<String, FxHashSet<usize>> = FxHashMap::default();
    let mut has_unguarded_write: FxHashSet<String> = FxHashSet::default();
    for node in semantic.nodes().iter() {
        let member = match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left
                else {
                    continue;
                };
                member.as_ref()
            }
            AstKind::UpdateExpression(update) => {
                let oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(member) =
                    &update.argument
                else {
                    continue;
                };
                member.as_ref()
            }
            _ => continue,
        };
        if member.property.name.as_str() != "current" {
            continue;
        }
        if member.span.start < body_span.start || member.span.end > body_span.end {
            continue;
        }
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            continue;
        };
        let name = obj.name.as_str();
        if !refs.contains(name) {
            continue;
        }
        if is_inside_nested_function(node.id(), body_span, semantic) {
            continue;
        }
        let containing_gate = gated.iter().position(|(_, consequent)| {
            member.span.start >= consequent.start && member.span.end <= consequent.end
        });
        match containing_gate {
            Some(idx) => {
                writes_into_gates.entry(name.to_string()).or_default().insert(idx);
            }
            None => {
                has_unguarded_write.insert(name.to_string());
            }
        }
    }

    let mut out: FxHashMap<String, Vec<ReinitGate>> = FxHashMap::default();
    for (name, gate_indices) in writes_into_gates {
        if has_unguarded_write.contains(&name) {
            continue;
        }
        let gates = gate_indices.iter().map(|&idx| gated[idx].0).collect();
        out.insert(name, gates);
    }
    out
}

/// Check if a `ref.current` member expression is the LHS of an
/// assignment (`ref.current = x`, `ref.current += x`, `ref.current ??= x`,
/// etc.) or the operand of an `UpdateExpression` (`ref.current++`,
/// `--ref.current`, etc.). The latest-ref pattern writes during render;
/// only reads are the antipattern. UpdateExpression cases are handled by a
/// dedicated visitor pass since they ARE read-then-write — we
/// just need to avoid double-flagging them here.
fn is_assignment_target(
    member_span: oxc_span::Span,
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_span::GetSpan;
    let nodes = semantic.nodes();
    // Walk up at most 3 parents to handle a parenthesised LHS like
    // `(ref.current) = x`, where the member sits under a
    // ParenthesizedExpression which sits under AssignmentExpression.
    let mut current = node_id;
    for _ in 0..3 {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::AssignmentExpression(assign) = parent.kind() {
            return assign.left.span().start == member_span.start
                && assign.left.span().end == member_span.end;
        }
        if let AstKind::UpdateExpression(update) = parent.kind() {
            return update.argument.span().start == member_span.start
                && update.argument.span().end == member_span.end;
        }
        current = parent_id;
    }
    false
}

/// Check if a node is inside a nested function (arrow, function expr/decl,
/// method) relative to the component body. If so, the `.current` read is OK.
fn is_inside_nested_function(
    node_id: oxc_semantic::NodeId,
    body_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current = node_id;
    loop {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            return false;
        }
        current = parent_id;
        let parent = nodes.get_node(current);
        // If we've reached above the body, stop
        let parent_span = match parent.kind() {
            AstKind::FunctionBody(b) => b.span,
            AstKind::Function(f) => f.span,
            AstKind::ArrowFunctionExpression(a) => a.span,
            _ => continue,
        };
        // If this function/arrow IS the component body itself, not nested
        if parent_span.start <= body_span.start && parent_span.end >= body_span.end {
            return false;
        }
        // Otherwise, we found a nested function
        if parent_span.start >= body_span.start && parent_span.end <= body_span.end {
            return true;
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useRef"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Find component/hook functions
        for node in semantic.nodes().iter() {
            let (name, body_span) = match node.kind() {
                AstKind::Function(func) => {
                    let Some(ident) = &func.id else { continue };
                    let name = ident.name.as_str().to_string();
                    let Some(body) = &func.body else { continue };
                    (name, body.span)
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    // Get name from parent VariableDeclarator
                    let parent_id = semantic.nodes().parent_id(node.id());
                    if parent_id == node.id() {
                        continue;
                    }
                    let parent = semantic.nodes().get_node(parent_id);
                    let AstKind::VariableDeclarator(decl) = parent.kind() else {
                        continue;
                    };
                    let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) =
                        &decl.id
                    else {
                        continue;
                    };
                    (ident.name.to_string(), arrow.body.span)
                }
                _ => continue,
            };

            if !starts_with_uppercase(&name) && !starts_with_use_hook(&name) {
                continue;
            }

            let refs = collect_ref_bindings(body_span, semantic, ctx.source);
            if refs.is_empty() {
                continue;
            }

            // Refs written ONLY in a post-mount effect (empty-dep
            // `useLayoutEffect`/`useEffect`) and initialized to a safe default
            // are never mutated during render; reading them during render is the
            // documented post-mount-measurement pattern and is safe.
            let safe_default_refs = collect_safe_default_refs(body_span, semantic, ctx.source);
            let post_mount_only_refs = collect_post_mount_effect_only_refs(
                body_span,
                &refs,
                &safe_default_refs,
                semantic,
                ctx.source,
            );

            // Refs following React's sanctioned lazy-init pattern: every
            // render-time write to `ref.current` is gated by `if (!ref.current)`
            // (or an equivalent nullish guard) on that same ref, so the write
            // runs once and subsequent render reads are stable — no tearing.
            let lazy_init_refs = collect_lazy_init_refs(body_span, &refs, semantic);

            // Refs following the controlled prop-change re-init pattern: every
            // render-time write is gated by an `if` whose `||` test combines a
            // nullish self-guard with change detectors on a ref's `.current`
            // (the react-hook-form `useForm` shape). Reads are exempt only when
            // provably stable — inside the gate test or strictly after the gate
            // block — so a read before the gate still flags.
            let reinit_gates = collect_controlled_reinit_refs(body_span, &refs, semantic);
            // A read is stable iff it is a guard read inside SOME own-gate test
            // (the change detector / nullish guard, which reads the prior stable
            // value to decide the re-init and never feeds render output), OR it
            // is strictly after EVERY own-gate block (all re-inits have run). A
            // read sitting before any own-gate — even after an earlier one — can
            // still observe a value a later gate then re-inits, so it tears and
            // stays flagged.
            let is_stable_reinit_read = |name: &str, read_span: oxc_span::Span| {
                reinit_gates.get(name).is_some_and(|gates| {
                    let inside_some_test = gates.iter().any(|gate| {
                        read_span.start >= gate.test.start && read_span.end <= gate.test.end
                    });
                    let after_every_gate =
                        gates.iter().all(|gate| read_span.start >= gate.stmt.end);
                    inside_some_test || after_every_gate
                })
            };

            // Walk semantic nodes for `.current` member accesses inside this body
            for inner_node in semantic.nodes().iter() {
                let AstKind::StaticMemberExpression(member) = inner_node.kind() else {
                    continue;
                };
                if member.property.name.as_str() != "current" {
                    continue;
                }
                // Must be inside the body
                if member.span.start < body_span.start || member.span.end > body_span.end {
                    continue;
                }
                // Object must be an identifier that's a ref
                let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
                    continue;
                };
                if !refs.contains(obj.name.as_str()) {
                    continue;
                }
                // Skip refs written only in a post-mount effect with a safe
                // default init — the render-time read cannot tear.
                if post_mount_only_refs.contains(obj.name.as_str()) {
                    continue;
                }
                // Skip refs following the sanctioned lazy-init pattern — every
                // render-time write is gated by a nullish guard on the ref, so
                // the read is stable after the one-time init.
                if lazy_init_refs.contains(obj.name.as_str()) {
                    continue;
                }
                // Skip reads of a controlled-re-init ref that are provably
                // stable (inside the gate test or after the gate block).
                if is_stable_reinit_read(obj.name.as_str(), member.span) {
                    continue;
                }
                // Must NOT be inside a nested function
                if is_inside_nested_function(inner_node.id(), body_span, semantic) {
                    continue;
                }
                // Skip writes to `ref.current` (latest-ref pattern, etc.).
                // Only reads of `ref.current` during render are flagged.
                if is_assignment_target(member.span, inner_node.id(), semantic) {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{}.current` is read during render — refs are designed for handlers and \
                         effects. Move the read into a handler or `useEffect`, or use state if you need \
                         the value during render.",
                        obj.name.as_str()
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }

            // Second pass: `ref.current++`, `--ref.current`, etc. An
            // `UpdateExpression` argument is typed as `SimpleAssignmentTarget`,
            // which does not surface as `AstKind::StaticMemberExpression` in
            // the semantic walk. These are read-then-write — same antipattern
            // as a plain read during render.
            for inner_node in semantic.nodes().iter() {
                let AstKind::UpdateExpression(update) = inner_node.kind() else {
                    continue;
                };
                let oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(member) =
                    &update.argument
                else {
                    continue;
                };
                if member.property.name.as_str() != "current" {
                    continue;
                }
                // Must be inside the body
                if update.span.start < body_span.start || update.span.end > body_span.end {
                    continue;
                }
                // Object must be an identifier that's a ref
                let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
                    continue;
                };
                if !refs.contains(obj.name.as_str()) {
                    continue;
                }
                // Must NOT be inside a nested function
                if is_inside_nested_function(inner_node.id(), body_span, semantic) {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, update.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{}.current` is read during render — refs are designed for handlers and \
                         effects. Move the read into a handler or `useEffect`, or use state if you need \
                         the value during render.",
                        obj.name.as_str()
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_ref_read_in_render() {
        let src =
            "function C() { const r = useRef(0); const v = r.current; return <div>{v}</div>; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_read_in_effect() {
        let src = "function C() { const r = useRef(0); useEffect(() => { console.log(r.current); }, []); return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_read_in_handler() {
        let src = "function C() { const r = useRef(0); return <button onClick={() => console.log(r.current)} />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_component_function() {
        let src = "function helper() { const r = useRef(0); return r.current; }";
        assert!(run(src).is_empty());
    }

    // Regression for issue #179 — latest-ref pattern: write during render is
    // not a read and must not be flagged.
    #[test]
    fn allows_latest_ref_pattern_assignment() {
        let src = "function MyComponent({ value, onChange }) { \
                   const valueRef = useRef(value); \
                   valueRef.current = value; \
                   useEffect(() => {}, []); \
                   return null; \
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_latest_ref_pattern_callback_assignment() {
        let src = "function MyComponent({ onChange }) { \
                   const onChangeRef = useRef(onChange); \
                   onChangeRef.current = onChange; \
                   return null; \
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_compound_assignment_to_ref_current() {
        let src = "function C() { const r = useRef(0); r.current += 1; return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_logical_assignment_to_ref_current() {
        let src = "function C({ value }) { const r = useRef(null); r.current ??= value; return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_read_in_variable_declaration() {
        let src = "function C() { const r = useRef(0); const v = r.current; return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_read_in_call_argument() {
        let src = "function C() { const r = useRef(0); console.log(r.current); return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_read_in_if_condition() {
        let src = "function C() { const r = useRef(0); if (r.current) { return null; } return null; }";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #179 — only the WRITE on the LHS should be
    // suppressed; the READ on the RHS still flags.
    #[test]
    fn still_flags_read_in_self_assignment_rhs() {
        let src = "function C() { const r = useRef(0); r.current = r.current + 1; return null; }";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #197 — UpdateExpression on `ref.current` is a
    // read-then-write during render and must be flagged. The argument of an
    // UpdateExpression is a SimpleAssignmentTarget, not surfaced as
    // StaticMemberExpression, so the original visitor missed it.
    #[test]
    fn flags_postfix_increment_on_ref_current() {
        let src = "function C() { const r = useRef(0); r.current++; return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_prefix_increment_on_ref_current() {
        let src = "function C() { const r = useRef(0); ++r.current; return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_postfix_decrement_on_ref_current() {
        let src = "function C() { const r = useRef(0); r.current--; return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_prefix_decrement_on_ref_current() {
        let src = "function C() { const r = useRef(0); --r.current; return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_update_on_ref_current_in_effect() {
        let src = "function C() { const r = useRef(0); useEffect(() => { r.current++; }, []); return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_update_on_non_ref_current() {
        let src = "function C() { const nonRef = { current: 0 }; nonRef.current++; return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_allows_plain_assignment_to_ref_current() {
        let src = "function C() { const r = useRef(0); r.current = 1; return null; }";
        assert!(run(src).is_empty());
    }

    // Regression for issue #2194 — canonical TanStack Virtual scroll-offset
    // pattern: a ref initialized to a safe default, written ONCE inside a
    // useLayoutEffect with empty deps, then read during render as a stable
    // layout config input. The ref is never mutated during render, so the read
    // cannot tear.
    #[test]
    fn allows_ref_read_when_written_only_in_layout_effect() {
        let src = "function Example() { \
                   const listRef = useRef(null); \
                   const listOffsetRef = useRef(0); \
                   useLayoutEffect(() => { listOffsetRef.current = listRef.current?.offsetTop ?? 0; }, []); \
                   const v = useWindowVirtualizer({ scrollMargin: listOffsetRef.current }); \
                   return <div ref={listRef}>{v}</div>; \
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_read_when_written_only_in_effect() {
        let src = "function Example() { \
                   const offsetRef = useRef(0); \
                   useEffect(() => { offsetRef.current = 42; }, []); \
                   return <div>{offsetRef.current}</div>; \
                   }";
        assert!(run(src).is_empty());
    }

    // Negative-space guard for #2194 — a ref written during render (not in an
    // effect) and then read during render is still the tearing antipattern and
    // must STILL be flagged. Only the WRITE is suppressed; the READ flags.
    #[test]
    fn still_flags_read_when_ref_written_during_render() {
        let src = "function C() { const r = useRef(0); r.current = compute(); return <div>{r.current}</div>; }";
        assert_eq!(run(src).len(), 1);
    }

    // Negative-space guard for #2194 — a ref read during render but written in
    // an effect with NON-empty deps re-runs after dependent renders, so the
    // render-time read can observe a stale/changing value. Still flagged.
    #[test]
    fn still_flags_read_when_effect_has_non_empty_deps() {
        let src = "function C({ dep }) { \
                   const r = useRef(0); \
                   useEffect(() => { r.current = dep; }, [dep]); \
                   return <div>{r.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 1);
    }

    // Negative-space guard for #2194 — a ref written BOTH during render and in
    // an effect must still be flagged: it is mutated during render.
    #[test]
    fn still_flags_read_when_ref_written_in_render_and_effect() {
        let src = "function C() { \
                   const r = useRef(0); \
                   r.current = 1; \
                   useEffect(() => { r.current = 2; }, []); \
                   return <div>{r.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #374 — latest-ref pattern with useCallback: the write
    // during render must not be flagged even when the ref is called inside a
    // useCallback handler with optional chaining.
    #[test]
    fn allows_latest_ref_write_with_usecallback_read() {
        let src = "function MyComponent({ value, onChange }) { \
                   const latestOnChange = useRef(onChange); \
                   latestOnChange.current = onChange; \
                   const handleClick = useCallback(() => { \
                     latestOnChange.current?.(value); \
                   }, [value]); \
                   return null; \
                   }";
        assert!(run(src).is_empty());
    }

    // Regression for issue #3990 — React's sanctioned lazy-ref-init pattern:
    // the write to `ref.current` is gated by `if (!ref.current)`, so it runs
    // once and the subsequent render read is stable. No tearing, no flag.
    #[test]
    fn allows_lazy_init_with_logical_not_guard() {
        let src = "function C() { \
                   const ref = useRef(null); \
                   if (!ref.current) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_lazy_init_with_strict_null_guard() {
        let src = "function C() { \
                   const ref = useRef(null); \
                   if (ref.current === null) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_lazy_init_with_strict_undefined_guard() {
        let src = "function C() { \
                   const ref = useRef(undefined); \
                   if (ref.current === undefined) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_lazy_init_with_loose_null_guard() {
        let src = "function C() { \
                   const ref = useRef(null); \
                   if (ref.current == null) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_lazy_init_with_void_guard() {
        let src = "function C() { \
                   const ref = useRef(undefined); \
                   if (ref.current === void 0) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert!(run(src).is_empty());
    }

    // Regression for #3990 (tearing FN) — a `||` guard with a non-nullish
    // operand (`!ref.current || other`) is NOT lazy-init: `other` lets the
    // write run on later renders, so the render read can tear. The ref is not
    // exempt; both reads (the `!ref.current` guard test and the render read)
    // flag.
    #[test]
    fn flags_or_guard_with_non_nullish_operand() {
        let src = "function C({ other }) { \
                   const ref = useRef(null); \
                   if (!ref.current || other) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 2);
    }

    // A `||` guard whose operands are BOTH nullish self-guards on the same ref
    // (`!ref.current || ref.current === undefined`) still gates a one-time
    // init, so the ref is exempt.
    #[test]
    fn allows_lazy_init_with_all_nullish_or_guard() {
        let src = "function C() { \
                   const ref = useRef(null); \
                   if (!ref.current || ref.current === undefined) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert!(run(src).is_empty());
    }

    // Regression for #3990 (tearing FN) — order-independent: a non-nullish
    // first operand (`cond || !ref.current`) also defeats the one-time gate.
    // Both reads (the `!ref.current` guard test and the render read) flag.
    #[test]
    fn flags_or_guard_with_non_nullish_operand_first() {
        let src = "function C({ cond }) { \
                   const ref = useRef(null); \
                   if (cond || !ref.current) { ref.current = compute(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 2);
    }

    // Negative-space guard for #3990 — a write guarded by an UNRELATED
    // condition (not a nullish guard on the ref) is not lazy-init: the write
    // may run on later renders, so the render read can still tear. Flag it.
    #[test]
    fn still_flags_read_when_write_guarded_by_unrelated_condition() {
        let src = "function C({ someProp }) { \
                   const ref = useRef(null); \
                   if (someProp) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 1);
    }

    // Negative-space guard for #3990 — a TRUTHY test `if (ref.current)` is the
    // OPPOSITE of a nullish guard (it writes only when already set), so it does
    // not gate a one-time init. The ref is not exempt: both reads (the `if`
    // test and the render read) still flag.
    #[test]
    fn still_flags_read_when_guarded_by_truthy_test() {
        let src = "function C() { \
                   const ref = useRef(null); \
                   if (ref.current) { ref.current = make(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 2);
    }

    // Regression for issue #4023 — the react-hook-form `useForm` controlled
    // prop-change re-init shape. The guard `if (!_formControl.current ||
    // (props.formControl && _formControlProp.current !== props.formControl))`
    // combines a nullish self-guard with a change detector on a ref's
    // `.current`: the gated writes run on first render or on a genuine prop
    // change, and every read sits inside the guard test or strictly after the
    // block, so the reads are stable within a render. No tearing — zero flags.
    #[test]
    fn rhf_useform_controlled_reinit_is_exempt() {
        let src = "function useForm(props) { \
                   const _formControl = React.useRef(undefined); \
                   const _formControlProp = React.useRef(props.formControl); \
                   if (!_formControl.current || (props.formControl && _formControlProp.current !== props.formControl)) { \
                     _formControlProp.current = props.formControl; \
                     _formControl.current = { ...rest, formState }; \
                   } \
                   const control = _formControl.current.control; \
                   _formControl.current.formState = React.useMemo(() => x, []); \
                   return _formControl.current; \
                   }";
        assert!(run(src).is_empty());
    }

    // FALSE-NEGATIVE GUARD for #4023 — the widening must NOT exempt an arbitrary
    // non-nullish disjunct. `if (cond || !ref.current)` re-runs the write every
    // render when `cond` is true, so a render read tears. `cond` is a bare
    // identifier, not a change detector on a ref's `.current`, so the `if` is
    // not a controlled re-init guard: the ref stays flagged (the `!ref.current`
    // guard read plus the render read).
    #[test]
    fn still_flags_or_guard_with_arbitrary_disjunct() {
        let src = "function C({ cond }) { \
                   const ref = useRef(null); \
                   if (cond || !ref.current) { ref.current = compute(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 2);
    }

    // FALSE-NEGATIVE GUARD for #4023 — an equality (`===`) disjunct is not a
    // change detector (inequality `!==`/`!=` only). `ref.current === x` is not
    // self-limiting, so the `if` is not a controlled re-init guard. Both the
    // guard reads (`!ref.current`, `ref.current === x`) and the render read
    // flag.
    #[test]
    fn still_flags_or_guard_with_equality_disjunct() {
        let src = "function C({ x }) { \
                   const ref = useRef(null); \
                   if (!ref.current || ref.current === x) { ref.current = compute(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 3);
    }

    // FALSE-NEGATIVE GUARD for #4023 — a non-nullish disjunct that is a `!==`
    // comparison but does NOT involve a ref's `.current` (`a !== b` on plain
    // values) is not a change detector, so the `if` is not a controlled re-init
    // guard and the ref stays flagged.
    #[test]
    fn still_flags_or_guard_with_non_ref_inequality() {
        let src = "function C({ a, b }) { \
                   const ref = useRef(null); \
                   if (!ref.current || a !== b) { ref.current = compute(); } \
                   return <div>{ref.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 2);
    }

    // FALSE-NEGATIVE GUARD for #4023 — a controlled re-init guard exempts reads
    // only when they are stable. A read positioned BEFORE the gating `if` block
    // can observe the pre-re-init value while a later read observes the
    // post-re-init value — tearing within the render — so the pre-gate read
    // still flags.
    #[test]
    fn still_flags_read_before_controlled_reinit_gate() {
        let src = "function useForm(props) { \
                   const _formControl = React.useRef(undefined); \
                   const _formControlProp = React.useRef(props.formControl); \
                   const early = _formControl.current; \
                   if (!_formControl.current || (props.formControl && _formControlProp.current !== props.formControl)) { \
                     _formControlProp.current = props.formControl; \
                     _formControl.current = { ...rest, formState }; \
                   } \
                   return _formControl.current; \
                   }";
        // The pre-gate `const early = _formControl.current` read flags; the
        // `return` read (after the block) is exempt.
        assert_eq!(run(src).len(), 1);
    }

    // FALSE-NEGATIVE GUARD for #4023 — each exempt ref is keyed ONLY to the
    // gate(s) that re-init it. A read of ref `b` positioned before `b`'s own
    // gate but after an UNRELATED earlier gate for ref `a` must still flag:
    // it observes `b`'s pre-re-init value while the later `return` read
    // observes the post-re-init value — tearing within the render.
    #[test]
    fn still_flags_read_before_own_gate_after_unrelated_gate() {
        let src = "function useForm(props) { \
                   const a = React.useRef(undefined); \
                   const b = React.useRef(undefined); \
                   const aProp = React.useRef(props.x); \
                   const bProp = React.useRef(props.y); \
                   if (!a.current || (props.x && aProp.current !== props.x)) { aProp.current = props.x; a.current = makeA(); } \
                   const tearing = b.current; \
                   if (!b.current || (props.y && bProp.current !== props.y)) { bProp.current = props.y; b.current = makeB(); } \
                   return null; \
                   }";
        // `const tearing = b.current` sits after `a`'s gate but before `b`'s
        // gate → not exempted by `b`'s own gate → flags. Exactly one diagnostic.
        assert_eq!(run(src).len(), 1);
    }

    // FALSE-NEGATIVE GUARD for #4023 — a ref re-initialized by TWO gates is
    // stable only after EVERY gate. A read sandwiched between the ref's two
    // gates observes the pre-re-init value of the second gate while a later
    // read observes the post-re-init value — tearing within the render — so the
    // sandwiched read still flags.
    #[test]
    fn still_flags_read_between_two_gates_for_same_ref() {
        let src = "function useForm(props) { \
                   const b = React.useRef(undefined); \
                   const bProp1 = React.useRef(props.x); \
                   const bProp2 = React.useRef(props.y); \
                   if (!b.current || (props.x && bProp1.current !== props.x)) { bProp1.current = props.x; b.current = makeB1(); } \
                   const tearing = b.current; \
                   if (!b.current || (props.y && bProp2.current !== props.y)) { bProp2.current = props.y; b.current = makeB2(); } \
                   return b.current; \
                   }";
        // `const tearing = b.current` sits after the first gate but before the
        // second gate (which can still re-init `b`) → flags. The `return` read
        // (after both gates) is exempt. Exactly one diagnostic.
        assert_eq!(run(src).len(), 1);
    }

    // Negative-space guard for #3990 — a ref written BOTH unconditionally and
    // inside a nullish-guarded `if` is mutated every render, so it is not
    // lazy-init. The ref is not exempt: both reads (the `!ref.current` guard
    // and the render read) still flag; only the write is suppressed.
    #[test]
    fn still_flags_read_when_ref_also_written_unconditionally() {
        let src = "function C() { \
                   const ref = useRef(null); \
                   if (!ref.current) { ref.current = make(); } \
                   ref.current = compute(); \
                   return <div>{ref.current}</div>; \
                   }";
        assert_eq!(run(src).len(), 2);
    }
}
