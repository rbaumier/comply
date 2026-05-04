use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const COLLECTIONS: &[&str] = &["Set", "Map", "WeakSet", "WeakMap"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toArray"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Must be `<expr>.toArray()` with zero arguments
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name != "toArray" {
            return;
        }
        if !call.arguments.is_empty() {
            return;
        }

        // Get the direct parent
        let parent = semantic.nodes().parent_node(node.id());
        let Some((msg, anchor_start, anchor_end)) =
            classify_context(parent, node, semantic)
        else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, anchor_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg.into(),
            severity: super::META.severity,
            span: Some((anchor_start as usize, (anchor_end - anchor_start) as usize)),
        });
    }
}

fn classify_context(
    parent: &oxc_semantic::AstNode,
    call_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<(&'static str, u32, u32)> {
    match parent.kind() {
        AstKind::ForOfStatement(stmt) => {
            Some((
                "`for...of` can iterate over an iterable, `.toArray()` is unnecessary.",
                stmt.span.start,
                stmt.span.end,
            ))
        }
        AstKind::SpreadElement(spread) => {
            Some((
                "Spread works on iterables, `.toArray()` is unnecessary.",
                spread.span.start,
                spread.span.end,
            ))
        }
        AstKind::YieldExpression(yield_expr) => {
            if yield_expr.delegate {
                Some((
                    "`yield*` can delegate to an iterable, `.toArray()` is unnecessary.",
                    yield_expr.span.start,
                    yield_expr.span.end,
                ))
            } else {
                None
            }
        }
        AstKind::NewExpression(new_expr) => {
            let Expression::Identifier(ctor) = &new_expr.callee else {
                return None;
            };
            if COLLECTIONS.contains(&ctor.name.as_str()) {
                return Some((
                    "Collection constructor accepts an iterable, `.toArray()` is unnecessary.",
                    new_expr.span.start,
                    new_expr.span.end,
                ));
            }
            None
        }
        AstKind::CallExpression(outer_call) => {
            let callee_text = match &outer_call.callee {
                Expression::StaticMemberExpression(m) => {
                    if let Expression::Identifier(obj) = &m.object {
                        Some(format!("{}.{}", obj.name, m.property.name))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let callee_text = callee_text?;
            if callee_text == "Array.from" {
                return Some((
                    "`Array.from()` accepts an iterable, `.toArray()` is unnecessary.",
                    outer_call.span.start,
                    outer_call.span.end,
                ));
            }
            if callee_text == "Object.fromEntries" {
                return Some((
                    "`Object.fromEntries()` accepts an iterable, `.toArray()` is unnecessary.",
                    outer_call.span.start,
                    outer_call.span.end,
                ));
            }
            None
        }
        // Intermediate wrapper — look at grandparent
        _ => {
            // Try grandparent for cases where there's an intermediate node
            let grandparent = semantic.nodes().parent_node(parent.id());
            classify_grandparent(grandparent, call_node, semantic)
        }
    }
}

fn classify_grandparent(
    node: &oxc_semantic::AstNode,
    _call_node: &oxc_semantic::AstNode,
    _semantic: &oxc_semantic::Semantic,
) -> Option<(&'static str, u32, u32)> {
    match node.kind() {
        AstKind::ForOfStatement(stmt) => {
            Some((
                "`for...of` can iterate over an iterable, `.toArray()` is unnecessary.",
                stmt.span.start,
                stmt.span.end,
            ))
        }
        AstKind::SpreadElement(spread) => {
            Some((
                "Spread works on iterables, `.toArray()` is unnecessary.",
                spread.span.start,
                spread.span.end,
            ))
        }
        AstKind::NewExpression(new_expr) => {
            let Expression::Identifier(ctor) = &new_expr.callee else {
                return None;
            };
            if COLLECTIONS.contains(&ctor.name.as_str()) {
                return Some((
                    "Collection constructor accepts an iterable, `.toArray()` is unnecessary.",
                    new_expr.span.start,
                    new_expr.span.end,
                ));
            }
            None
        }
        AstKind::CallExpression(outer_call) => {
            let callee_text = match &outer_call.callee {
                Expression::StaticMemberExpression(m) => {
                    if let Expression::Identifier(obj) = &m.object {
                        Some(format!("{}.{}", obj.name, m.property.name))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let callee_text = callee_text?;
            if callee_text == "Array.from" {
                return Some((
                    "`Array.from()` accepts an iterable, `.toArray()` is unnecessary.",
                    outer_call.span.start,
                    outer_call.span.end,
                ));
            }
            if callee_text == "Object.fromEntries" {
                return Some((
                    "`Object.fromEntries()` accepts an iterable, `.toArray()` is unnecessary.",
                    outer_call.span.start,
                    outer_call.span.end,
                ));
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_for_of_to_array() {
        let d = run_on("for (const x of iter.toArray()) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("for...of"));
    }

    #[test]
    fn flags_spread_to_array() {
        let d = run_on("const arr = [...iter.toArray()];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Spread"));
    }

    #[test]
    fn flags_new_set_to_array() {
        let d = run_on("const s = new Set(iter.toArray());");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Collection"));
    }

    #[test]
    fn flags_array_from_to_array() {
        let d = run_on("const a = Array.from(iter.toArray());");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn allows_standalone_to_array() {
        assert!(run_on("const arr = iter.toArray();").is_empty());
    }

    #[test]
    fn allows_non_to_array_method() {
        assert!(run_on("for (const x of iter.values()) {}").is_empty());
    }
}
