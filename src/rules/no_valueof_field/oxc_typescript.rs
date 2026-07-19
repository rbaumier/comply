//! OxcCheck backend for no-valueof-field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["valueOf"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                // Class method: class Foo { valueOf() {} }
                AstKind::MethodDefinition(method) => {
                    if let PropertyKey::StaticIdentifier(id) = &method.key
                        && id.name == "valueOf" {
                            push(&mut diagnostics, ctx, id.span);
                        }
                }
                // Interface method signature: interface Foo { valueOf(): number }
                AstKind::TSMethodSignature(sig) => {
                    if let PropertyKey::StaticIdentifier(id) = &sig.key
                        && id.name == "valueOf"
                        && !in_structural_type_literal(node.id(), semantic) {
                            push(&mut diagnostics, ctx, id.span);
                        }
                }
                // Interface/type property signature: interface Foo { valueOf: () => number }
                AstKind::TSPropertySignature(sig) => {
                    if let PropertyKey::StaticIdentifier(id) = &sig.key
                        && id.name == "valueOf"
                        && !in_structural_type_literal(node.id(), semantic) {
                            push(&mut diagnostics, ctx, id.span);
                        }
                }
                // Object property: { valueOf: function() {} } or { valueOf: () => {} }
                AstKind::ObjectProperty(prop) => {
                    if let PropertyKey::StaticIdentifier(id) = &prop.key
                        && id.name == "valueOf" {
                            // Only flag if value is a function.
                            if prop.method
                                || matches!(
                                    prop.value,
                                    Expression::ArrowFunctionExpression(_)
                                        | Expression::FunctionExpression(_)
                                )
                            {
                                push(&mut diagnostics, ctx, id.span);
                            }
                        }
                }
                // Class field: class Foo { valueOf = () => 1 }
                AstKind::PropertyDefinition(field) => {
                    if let PropertyKey::StaticIdentifier(id) = &field.key
                        && id.name == "valueOf" {
                            push(&mut diagnostics, ctx, id.span);
                        }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

/// Returns true when the signature's nearest enclosing type container is an
/// inline `TSTypeLiteral` rather than an `interface` body.
///
/// `{ valueOf(): number }` in a type-annotation position (parameter type,
/// type-alias RHS, union member) is a structural contract — "accepts any
/// number-coercible value" — not a `valueOf` field/method definition, so it
/// must not be flagged. Members of a named `interface` remain flagged.
fn in_structural_type_literal(node_id: oxc_semantic::NodeId, semantic: &oxc_semantic::Semantic) -> bool {
    semantic
        .nodes()
        .ancestors(node_id)
        .find_map(|ancestor| match ancestor.kind() {
            AstKind::TSTypeLiteral(_) => Some(true),
            AstKind::TSInterfaceBody(_) => Some(false),
            _ => None,
        })
        .unwrap_or(false)
}

fn push(diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx, span: oxc_span::Span) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Do not override `valueOf`. Use an explicit conversion method instead.".into(),
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_valueof_in_structural_type_annotation() {
        // Regression #4780: `{ valueOf(): number }` in a type-annotation
        // position is a structural contract, not a `valueOf` definition.
        let src = "export type ColorScale = (count: number | { valueOf(): number }) => string | undefined;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_valueof_property_signature_in_structural_type_literal() {
        let src = "export type Coercible = { valueOf: () => number };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_valueof_in_named_interface() {
        let src = "export interface Coercible { valueOf(): number; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_valueof_method_in_class() {
        let src = "class Money { valueOf() { return 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_valueof_in_object_literal() {
        let src = "const m = { valueOf() { return 1; } };";
        assert_eq!(run_on(src).len(), 1);
    }
}
