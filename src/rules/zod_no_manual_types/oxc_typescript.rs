//! OxcCheck backend for zod-no-manual-types.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.infer"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // 1. Collect key sets from z.object({...}) calls.
        let mut schema_key_sets: Vec<BTreeSet<String>> = Vec::new();
        // 2. Collect type alias declarations to check.
        let mut type_aliases: Vec<(&TSTypeAliasDeclaration, BTreeSet<String>)> = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::CallExpression(call) => {
                    // Check for z.object(...)
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if member.property.name == "object" {
                            if let Expression::Identifier(id) = &member.object {
                                if id.name == "z" {
                                    if let Some(first_arg) = call.arguments.first() {
                                        if let Some(expr) = first_arg.as_expression() {
                                            if let Some(keys) = collect_object_expr_keys(expr, ctx.source) {
                                                schema_key_sets.push(keys);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                AstKind::TSTypeAliasDeclaration(alias) => {
                    // Check the alias value for object type keys.
                    if let Some(keys) = collect_ts_type_keys(&alias.type_annotation) {
                        // Check that the alias source does not contain z.infer.
                        let alias_text = &ctx.source[alias.span.start as usize..alias.span.end as usize];
                        if !alias_text.contains("z.infer") {
                            type_aliases.push((alias, keys));
                        }
                    }
                }
                _ => {}
            }
        }

        if schema_key_sets.is_empty() {
            return diagnostics;
        }

        for (alias, keys) in &type_aliases {
            if schema_key_sets.iter().any(|s| s == keys) {
                let (line, column) = byte_offset_to_line_col(ctx.source, alias.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "This `type` alias duplicates a Zod schema in the same file — \
                              use `z.infer<typeof Schema>` instead so the type stays in sync."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

fn collect_object_expr_keys(expr: &Expression, _source: &str) -> Option<BTreeSet<String>> {
    let Expression::ObjectExpression(obj) = expr else { return None };
    let mut keys = BTreeSet::new();
    for prop in &obj.properties {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            let key_text = match &p.key {
                PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
                PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
                _ => None,
            };
            if let Some(k) = key_text {
                keys.insert(k);
            }
        }
    }
    if keys.is_empty() { None } else { Some(keys) }
}

fn collect_ts_type_keys(ty: &TSType) -> Option<BTreeSet<String>> {
    let TSType::TSTypeLiteral(lit) = ty else { return None };
    let mut keys = BTreeSet::new();
    for member in &lit.members {
        if let TSSignature::TSPropertySignature(prop) = member {
            let key_text = match &prop.key {
                PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
                PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
                _ => None,
            };
            if let Some(k) = key_text {
                keys.insert(k);
            }
        }
    }
    if keys.is_empty() { None } else { Some(keys) }
}
