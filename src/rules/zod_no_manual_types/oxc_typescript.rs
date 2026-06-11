//! OxcCheck backend for zod-no-manual-types.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
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
                    if let Expression::StaticMemberExpression(member) = &call.callee
                        && member.property.name == "object"
                            && let Expression::Identifier(id) = &member.object
                                && id.name == "z"
                                    && is_named_schema_init(node, semantic)
                                    && let Some(first_arg) = call.arguments.first()
                                        && let Some(expr) = first_arg.as_expression()
                                            && let Some(keys) = collect_object_expr_keys(expr, ctx.source) {
                                                schema_key_sets.push(keys);
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

/// Returns true when the `z.object(...)` call is the initializer of a
/// `VariableDeclarator` (`const X = z.object({...})`), possibly through a
/// method chain such as `z.object({...}).strict()`. Only such schemas are
/// referenceable via `z.infer<typeof X>`; an anonymous `z.object(...)` nested
/// inside another schema's arguments is not a candidate.
fn is_named_schema_init(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let mut current_span = node.kind().span();
    loop {
        let parent = nodes.parent_node(current_id);
        if parent.id() == current_id {
            return false;
        }
        match parent.kind() {
            AstKind::VariableDeclarator(_) => return true,
            // `.strict()` / `.partial()` / ... chained on the schema: keep
            // climbing while we stay on the callee side of the chain.
            AstKind::StaticMemberExpression(member) if member.object.span() == current_span => {}
            AstKind::CallExpression(call) if call.callee.span() == current_span => {}
            _ => return false,
        }
        current_id = parent.id();
        current_span = parent.kind().span();
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_duplicate_of_named_schema() {
        let src = "const Foo = z.object({ a: z.string(), b: z.number() });\n\
                   type Bar = { a: string; b: number };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_duplicate_of_chained_named_schema() {
        let src = "const Foo = z.object({ a: z.string(), b: z.number() }).strict();\n\
                   type Bar = { a: string; b: number };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_duplicate_of_named_inner_schema() {
        let src = "const Inner = z.object({ statut: z.string(), page: z.number() });\n\
                   const Outer = z.object({ search: Inner, replace: z.boolean() });\n\
                   type C = { statut: string; page: number };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_z_infer_alias() {
        let src = "const Foo = z.object({ a: z.string(), b: z.number() });\n\
                   type Bar = z.infer<typeof Foo>;";
        assert!(run(src).is_empty());
    }

    // Regression for #965: a flat alias that only shares field names with an
    // anonymous `z.object(...)` nested inside another schema must not be
    // flagged — that inner schema has no name, so `z.infer<typeof X>` cannot
    // reproduce the alias.
    #[test]
    fn ignores_flat_projection_of_anonymous_nested_schema() {
        let src = r#"
import { z } from "zod";

const StatutNavigateCallSchema = z.object({
  search: z.object({
    statut: z.enum(["actif", "tous", "desactive"]),
    page: z.number(),
  }),
  replace: z.boolean(),
});

type ParsedStatutNavigateCall = z.infer<typeof StatutNavigateCallSchema>;

type StatutNavigateCriteria = { statut: Statut; page: number };
"#;
        assert!(run(src).is_empty());
    }
}
