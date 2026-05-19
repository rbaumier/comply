//! api-branded-id-types OxcCheck backend — flag function parameters named
//! `*Id` / `*_id` typed as bare `string` or `number` in exported functions.
//!
//! Relaxation: when the parameter is used exclusively as an equality
//! comparison operand inside the function body (and never returned, stored,
//! or passed on), the rule does not flag — the value flows nowhere
//! downstream so the brand would buy nothing.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, BinaryOperator, TSType};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FormalParameter(param) = node.kind() else {
            return;
        };

        // Extract parameter name
        let BindingPattern::BindingIdentifier(ident) = &param.pattern else {
            return;
        };
        let name = ident.name.as_str();
        if !name_looks_like_id(name) {
            return;
        }

        // Check type annotation is bare `string` or `number`
        let Some(type_ann) = &param.type_annotation else {
            return;
        };
        let Some(kind) = bare_primitive_kind(&type_ann.type_annotation) else {
            return;
        };

        // Check if in exported context
        if !is_in_exported_context(node.id(), semantic) {
            return;
        }

        // Relaxation: if the parameter is only used as an equality-comparison
        // operand inside the function body (never returned, stored, passed on,
        // or read for any other purpose), the brand provides no extra safety
        // — the value flows nowhere downstream. This matches the common case
        // of filtering by an ID coming from a third-party type that widens
        // to plain `string` (e.g. Better Auth's `session.userId: string`).
        if is_comparison_only_usage(ident.symbol_id.get(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Parameter `{name}: {kind}` uses a raw primitive — use a branded ID type so unrelated IDs can't be swapped at call sites."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn name_looks_like_id(name: &str) -> bool {
    if name == "id" {
        return true;
    }
    if name.ends_with("_id") && name.len() > 3 {
        return true;
    }
    // camelCase: ends with "Id" and preceded by lowercase
    if name.ends_with("Id") && name.len() > 2 {
        let prev = name.as_bytes()[name.len() - 3];
        if prev.is_ascii_lowercase() {
            return true;
        }
    }
    false
}

fn bare_primitive_kind(ts_type: &TSType<'_>) -> Option<&'static str> {
    match ts_type {
        TSType::TSStringKeyword(_) => Some("string"),
        TSType::TSNumberKeyword(_) => Some("number"),
        _ => None,
    }
}

/// Returns `true` when every resolved reference to `symbol_id` is the direct
/// operand of an equality comparison (`===`, `!==`, `==`, `!=`) and there is
/// at least one such reference. Parenthesised wrappers are transparent.
///
/// When `symbol_id` is `None` (no resolved binding), returns `false` so the
/// caller falls back to flagging.
fn is_comparison_only_usage(
    symbol_id: Option<oxc_semantic::SymbolId>,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let Some(symbol_id) = symbol_id else {
        return false;
    };
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();

    let mut saw_reference = false;
    for reference in scoping.get_resolved_references(symbol_id) {
        saw_reference = true;
        let ref_id = reference.node_id();
        if !is_equality_operand(ref_id, nodes) {
            return false;
        }
    }
    saw_reference
}

/// Walk parents past any `ParenthesizedExpression` and return `true` if the
/// first non-parenthesised ancestor is a `BinaryExpression` whose operator is
/// `===`, `!==`, `==`, or `!=`.
fn is_equality_operand(ref_id: oxc_semantic::NodeId, nodes: &oxc_semantic::AstNodes) -> bool {
    let mut current = ref_id;
    loop {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            return false;
        }
        match nodes.kind(parent_id) {
            AstKind::ParenthesizedExpression(_) => {
                current = parent_id;
            }
            AstKind::BinaryExpression(bin) => {
                return matches!(
                    bin.operator,
                    BinaryOperator::Equality
                        | BinaryOperator::StrictEquality
                        | BinaryOperator::Inequality
                        | BinaryOperator::StrictInequality
                );
            }
            _ => return false,
        }
    }
}

fn is_in_exported_context(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node_id;

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            AstKind::Function(_) => {
                // Check if this function is exported
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id
                    && let AstKind::ExportNamedDeclaration(_) = nodes.get_node(gp_id).kind() {
                        return true;
                    }
                return false;
            }
            AstKind::ArrowFunctionExpression(_) => {
                // Check parent chain: VariableDeclarator -> VariableDeclaration -> ExportNamedDeclaration
                let mut up_id = nodes.parent_id(parent_id);
                loop {
                    if up_id == nodes.parent_id(up_id) {
                        return false;
                    }
                    match nodes.get_node(up_id).kind() {
                        AstKind::VariableDeclarator(_)
                        | AstKind::VariableDeclaration(_) => {
                            up_id = nodes.parent_id(up_id);
                        }
                        AstKind::ExportNamedDeclaration(_) => return true,
                        _ => return false,
                    }
                }
            }
            AstKind::MethodDefinition(_) => {
                // Check if the enclosing class is exported
                let mut up_id = nodes.parent_id(parent_id);
                loop {
                    if up_id == nodes.parent_id(up_id) {
                        return false;
                    }
                    match nodes.get_node(up_id).kind() {
                        AstKind::ClassBody(_) => {
                            up_id = nodes.parent_id(up_id);
                        }
                        AstKind::Class(_) => {
                            let class_parent_id = nodes.parent_id(up_id);
                            if class_parent_id != up_id
                                && let AstKind::ExportNamedDeclaration(_) =
                                    nodes.get_node(class_parent_id).kind()
                                {
                                    return true;
                                }
                            return false;
                        }
                        _ => return false,
                    }
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_raw_string_id_in_exported_function() {
        let d = run("export function getOrder(orderId: string) { return orderId; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("orderId"));
    }

    #[test]
    fn allows_branded_id_type() {
        assert!(run("export function getOrder(orderId: OrderId) { return orderId; }").is_empty());
    }

    // --- Issue #184 regression: comparison-only usage ---

    #[test]
    fn allows_comparison_only_usage_in_exported_function() {
        // The user's exact repro from issue #184.
        let src = r#"
            export function invalidateCachedSessionsByUserId(userId: string): void {
                for (const [key, entry] of cache) {
                    if (entry.data.session.userId === userId) {
                        cache.delete(key);
                    }
                }
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_comparison_only_usage_with_loose_equality() {
        let src = r#"
            export function matchById(userId: string): boolean {
                return current.userId == userId;
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_comparison_only_usage_with_inequality() {
        let src = r#"
            export function differs(userId: string): boolean {
                return other.userId !== userId;
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_comparison_only_usage_with_parenthesised_operand() {
        let src = r#"
            export function check(userId: string): boolean {
                return ((entry.userId) === (userId));
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_when_parameter_is_returned() {
        let src = r#"
            export function check(userId: string): string {
                if (entry.userId === userId) {
                    return userId;
                }
                return "";
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_parameter_is_stored_on_object() {
        let src = r#"
            export function check(userId: string): void {
                obj.id = userId;
                if (entry.userId === userId) {
                    return;
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_parameter_is_passed_to_another_function() {
        let src = r#"
            export function check(userId: string): void {
                doStuff(userId);
                if (entry.userId === userId) {
                    return;
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_parameter_is_used_in_template_literal() {
        let src = r#"
            export function check(userId: string): void {
                log(`looking up ${userId}`);
                if (entry.userId === userId) {
                    return;
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_parameter_is_member_accessed() {
        // A string parameter has no members, but the user might have widened
        // its type elsewhere. Member access still escapes "pure comparison".
        let src = r#"
            export function check(userId: string): number {
                return userId.length;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_simple_positive_case_with_find() {
        let src = r#"
            export function load(userId: string) {
                return db.users.find(userId);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
