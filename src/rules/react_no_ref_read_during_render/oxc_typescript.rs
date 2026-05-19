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

/// Check if a `ref.current` member expression is the LHS of an
/// assignment (`ref.current = x`, `ref.current += x`, `ref.current ??= x`,
/// etc.) or the operand of an `UpdateExpression` (`ref.current++`,
/// `--ref.current`, etc.). The latest-ref pattern writes during render;
/// only reads are the antipattern. UpdateExpression cases are handled by a
/// dedicated visitor pass (issue #197) since they ARE read-then-write — we
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
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
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
}
