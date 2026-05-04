//! api-separate-input-output-types OXC backend.
//!
//! Walk interface / type-alias declarations. Flag if the type contains
//! server-managed fields and is used in both input and output positions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{TSSignature, TSType, TSTypeName};
use std::collections::HashSet;
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

/// Collect type identifier names from a type annotation subtree.
fn collect_type_names_from_ts_type(ts_type: &TSType, out: &mut HashSet<String>) {
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

        // Pass 1: collect input/output type positions
        let mut inputs: HashSet<String> = HashSet::new();
        let mut outputs: HashSet<String> = HashSet::new();

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
                    if !should_flag(name, &inputs, &outputs) {
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
                    if !should_flag(name, &inputs, &outputs) {
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

fn should_flag(name: &str, inputs: &HashSet<String>, outputs: &HashSet<String>) -> bool {
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
