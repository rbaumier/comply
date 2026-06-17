//! api-separate-input-output-types OXC backend.
//!
//! Walk interface / type-alias declarations. Flag an *exported* type that
//! contains server-managed fields and is used in both input and output
//! positions. Non-exported, file-local helper types never reach an API
//! boundary, so they are skipped.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, ExportNamedDeclaration, TSSignature, TSType, TSTypeName};
use rustc_hash::FxHashSet;
use std::sync::Arc;

const SERVER_MANAGED_FIELDS: &[&str] = &[
    "id",
    "createdAt",
    "updatedAt",
    "created_at",
    "updated_at",
    "deletedAt",
    "deleted_at",
];

const OUTPUT_SUFFIXES: &[&str] = &[
    "Response", "Output", "Dto", "DTO", "Result", "Reply", "Payload", "View", "Entity", "Model",
    "Row", "Record",
];

const INPUT_SUFFIXES: &[&str] = &[
    "Input", "Request", "Create", "Update", "Patch", "Args", "Params", "Body",
];

fn has_output_suffix(name: &str) -> bool {
    OUTPUT_SUFFIXES.iter().any(|s| name.ends_with(s))
}

fn has_input_suffix(name: &str) -> bool {
    INPUT_SUFFIXES.iter().any(|s| name.ends_with(s))
}

fn collect_server_fields<'a>(members: &'a [TSSignature<'a>]) -> Vec<&'a str> {
    members
        .iter()
        .filter_map(|sig| {
            if let TSSignature::TSPropertySignature(prop) = sig
                && let oxc_ast::ast::PropertyKey::StaticIdentifier(ident) = &prop.key {
                    let name = ident.name.as_str();
                    if SERVER_MANAGED_FIELDS.contains(&name) {
                        return Some(name);
                    }
                }
            None
        })
        .collect()
}

/// Collect the names of type/interface declarations that this `export`
/// statement makes part of the module's public surface — both inline
/// (`export interface X`, `export type X = …`) and named re-exports
/// (`export { X }`, `export type { X }`).
fn collect_exported_type_names(export: &ExportNamedDeclaration, out: &mut FxHashSet<String>) {
    match &export.declaration {
        Some(Declaration::TSInterfaceDeclaration(decl)) => {
            out.insert(decl.id.name.to_string());
        }
        Some(Declaration::TSTypeAliasDeclaration(decl)) => {
            out.insert(decl.id.name.to_string());
        }
        _ => {}
    }
    for spec in &export.specifiers {
        out.insert(spec.local.name().to_string());
    }
}

/// Collect type identifier names from a type annotation subtree.
fn collect_type_names_from_ts_type(ts_type: &TSType, out: &mut FxHashSet<String>) {
    match ts_type {
        TSType::TSTypeReference(tref) => {
            if let TSTypeName::IdentifierReference(ident) = &tref.type_name {
                out.insert(ident.name.to_string());
            }
            if let Some(args) = &tref.type_arguments {
                for arg in &args.params {
                    collect_type_names_from_ts_type(arg, out);
                }
            }
        }
        TSType::TSUnionType(u) => {
            for t in &u.types {
                collect_type_names_from_ts_type(t, out);
            }
        }
        TSType::TSIntersectionType(i) => {
            for t in &i.types {
                collect_type_names_from_ts_type(t, out);
            }
        }
        TSType::TSArrayType(a) => {
            collect_type_names_from_ts_type(&a.element_type, out);
        }
        _ => {}
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[] // full-program analysis
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let nodes = semantic.nodes();

        // Pass 1: collect input/output type positions and exported type names.
        let mut inputs: FxHashSet<String> = FxHashSet::default();
        let mut outputs: FxHashSet<String> = FxHashSet::default();
        let mut exported: FxHashSet<String> = FxHashSet::default();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::FormalParameter(param) => {
                    if let Some(ta) = &param.type_annotation {
                        collect_type_names_from_ts_type(&ta.type_annotation, &mut inputs);
                    }
                }
                AstKind::Function(func) => {
                    if let Some(rt) = &func.return_type {
                        collect_type_names_from_ts_type(&rt.type_annotation, &mut outputs);
                    }
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if let Some(rt) = &arrow.return_type {
                        collect_type_names_from_ts_type(&rt.type_annotation, &mut outputs);
                    }
                }
                AstKind::ExportNamedDeclaration(export) => {
                    collect_exported_type_names(export, &mut exported);
                }
                _ => {}
            }
        }

        // Pass 2: check declarations
        let mut diagnostics = Vec::new();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::TSInterfaceDeclaration(decl) => {
                    let name = decl.id.name.as_str();
                    let server_fields = collect_server_fields(&decl.body.body);
                    if server_fields.is_empty() {
                        continue;
                    }
                    if !should_flag(name, &inputs, &outputs, &exported) {
                        continue;
                    }
                    let joined = server_fields.join(", ");
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, decl.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Type `{name}` mixes server-managed fields ({joined}) with other fields; split into separate input/output types so clients don't own server-assigned values."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                AstKind::TSTypeAliasDeclaration(decl) => {
                    let TSType::TSTypeLiteral(lit) = &decl.type_annotation else {
                        continue;
                    };
                    let name = decl.id.name.as_str();
                    let server_fields = collect_server_fields(&lit.members);
                    if server_fields.is_empty() {
                        continue;
                    }
                    if !should_flag(name, &inputs, &outputs, &exported) {
                        continue;
                    }
                    let joined = server_fields.join(", ");
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, decl.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Type `{name}` mixes server-managed fields ({joined}) with other fields; split into separate input/output types so clients don't own server-assigned values."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn should_flag(
    name: &str,
    inputs: &FxHashSet<String>,
    outputs: &FxHashSet<String>,
    exported: &FxHashSet<String>,
) -> bool {
    // Non-exported types are module-internal and never cross an API
    // boundary: a local helper used only as a function param / return type
    // is not a request/response DTO, so the input/output split doesn't apply.
    if !exported.contains(name) {
        return false;
    }
    let used_in = inputs.contains(name);
    let used_out = outputs.contains(name);
    if has_input_suffix(name) {
        used_in
    } else if has_output_suffix(name) {
        false
    } else {
        // bare entity: flag only when used in BOTH positions
        used_in && used_out
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
    ) -> Vec<Diagnostic> {
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
    fn flags_exported_input_type_with_server_fields_when_used_as_param() {
        let d = run(
            "export interface CreateUserInput { id: string; name: string; createdAt: string }\n\
             function create(input: CreateUserInput) { return input; }",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("CreateUserInput"));
    }

    #[test]
    fn flags_exported_bare_entity_used_as_both_input_and_output() {
        let d = run(
            "export interface User { id: string; name: string; createdAt: string }\n\
             function save(u: User): User { return u; }",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_when_exported_via_separate_export_clause() {
        let d = run(
            "interface User { id: string; name: string; createdAt: string }\n\
             function save(u: User): User { return u; }\n\
             export { User };",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_exported_internal_type_used_as_param_and_return() {
        // Regression for #4041: a non-exported, file-local helper type that
        // never reaches an API boundary — used only as a function param type
        // and an internal Result return type — is not a request/response DTO
        // and must not be flagged.
        assert!(
            run(
                "type TargetTeam = { id: TeamId; organizationId: OrganizationId };\n\
                 const toAuthorizeIntent = (targetTeams: TargetTeam[]) => targetTeams;\n\
                 async function resolveTargetTeams(): Promise<Result<TargetTeam[], ApiError>> {\n\
                   return ok([]);\n\
                 }",
            )
            .is_empty()
        );
    }
}
