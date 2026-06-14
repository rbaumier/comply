//! react-no-ref-read-during-render OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::collections::HashSet;
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
) -> HashSet<String> {
    let mut refs = HashSet::new();
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
) -> HashSet<String> {
    let mut refs = HashSet::new();
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
    refs: &HashSet<String>,
    safe_default_refs: &HashSet<String>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> HashSet<String> {
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
    let mut written_in_render: HashSet<String> = HashSet::new();
    let mut written_in_effect: HashSet<String> = HashSet::new();

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
}
