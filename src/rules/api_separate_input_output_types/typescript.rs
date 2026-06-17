//! Walk interface / type-alias declarations. Only *exported* types can
//! reach an API boundary; non-exported, file-local helper types are
//! skipped. For an exported declaration that contains server-managed
//! fields (`id`, `createdAt`, `updatedAt`) AND is used in BOTH input and
//! output positions in the same file, flag it.
//!
//! "Input position": parameter type annotation, request body type
//! argument, or `Body<T>` / `Request<...,...,T>`-style generic.
//! "Output position": function/arrow return type annotation, or
//! `Response<T>` / `Promise<T>`-style return wrapper.
//!
//! For names with explicit `*Input`/`*Request` suffix we require only
//! that the type appears in some parameter/input position; the suffix
//! itself signals "input intent" so we don't need an output sighting.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::FxHashSet;

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

fn collect_prop_names<'a>(
    body: tree_sitter::Node<'a>,
    source: &'a [u8],
    out: &mut Vec<(String, tree_sitter::Node<'a>)>,
) {
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() != "property_signature" {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()]) else {
            continue;
        };
        out.push((name.to_string(), member));
    }
}

fn check_decl(
    node: tree_sitter::Node,
    type_name: &str,
    body: tree_sitter::Node,
    source: &[u8],
    ctx_path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut props = Vec::new();
    collect_prop_names(body, source, &mut props);

    let server_fields: Vec<&str> = props
        .iter()
        .filter(|(n, _)| SERVER_MANAGED_FIELDS.contains(&n.as_str()))
        .map(|(n, _)| n.as_str())
        .collect();

    if server_fields.is_empty() {
        return;
    }

    // Only flag types whose name signals input use, OR bare entity names
    // (no output suffix) that embed server-managed fields.
    let is_input_named = has_input_suffix(type_name);
    let is_bare_entity = !has_output_suffix(type_name) && !has_input_suffix(type_name);

    if !is_input_named && !is_bare_entity {
        return;
    }

    let joined = server_fields.join(", ");
    diagnostics.push(Diagnostic::at_node(
        ctx_path,
        &node,
        super::META.id,
        format!(
            "Type `{type_name}` mixes server-managed fields ({joined}) with other fields; split into separate input/output types so clients don't own server-assigned values."
        ),
        Severity::Warning,
    ));
}

/// Decide whether a declared type qualifies for the input/output split
/// check. Non-exported types are module-internal and never cross an API
/// boundary, so a local helper used only as a function param / return type
/// is not a request/response DTO and is never flagged.
fn type_qualifies(
    name: &str,
    inputs: &FxHashSet<String>,
    outputs: &FxHashSet<String>,
    exported: &FxHashSet<String>,
) -> bool {
    if !exported.contains(name) {
        return false;
    }
    let used_in = inputs.contains(name);
    let used_out = outputs.contains(name);
    if has_input_suffix(name) {
        // Explicit input naming: a sighting in a parameter is enough.
        used_in
    } else {
        // Bare entity / output-suffix names only qualify when the type
        // is used in BOTH positions.
        used_in && used_out
    }
}

/// Collect the names of type/interface declarations exported from this
/// module — both inline (`export interface X`, `export type X = …`) and
/// named re-exports (`export { X }`, `export type { X }`).
fn collect_exported_type_names(
    program: tree_sitter::Node<'_>,
    source: &[u8],
) -> FxHashSet<String> {
    let mut exported: FxHashSet<String> = FxHashSet::default();

    let mut cursor = program.walk();
    for child in program.children(&mut cursor) {
        if child.kind() != "export_statement" {
            continue;
        }
        // Inline `export interface X` / `export type X = …`.
        if let Some(decl) = child.child_by_field_name("declaration")
            && matches!(
                decl.kind(),
                "interface_declaration" | "type_alias_declaration"
            )
            && let Some(name_node) = decl.child_by_field_name("name")
            && let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()])
        {
            exported.insert(name.to_string());
        }
        // Named re-exports `export { X }` / `export type { X }`: collect
        // every exported local identifier.
        collect_export_specifier_names(child, source, &mut exported);
    }
    exported
}

/// Collect the local names of every `export_specifier` within an
/// `export_statement`'s `export_clause` (e.g. the `X` in `export { X }`).
fn collect_export_specifier_names(
    export_statement: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut FxHashSet<String>,
) {
    let mut cursor = export_statement.walk();
    let root_id = export_statement.id();
    loop {
        let n = cursor.node();
        if n.kind() == "export_specifier"
            && let Some(name_node) = n.child_by_field_name("name")
            && let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()])
        {
            out.insert(name.to_string());
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.node().id() == root_id {
                return;
            }
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

/// Walk the whole program once and collect, for every type identifier
/// reference, whether it appeared in an input position (parameter type
/// annotation) or in an output position (function return type).
fn collect_type_positions(
    program: tree_sitter::Node<'_>,
    source: &[u8],
) -> (FxHashSet<String>, FxHashSet<String>) {
    let mut inputs: FxHashSet<String> = FxHashSet::default();
    let mut outputs: FxHashSet<String> = FxHashSet::default();

    let mut cursor = program.walk();
    let root_id = program.id();
    loop {
        let n = cursor.node();
        match n.kind() {
            // Parameter annotation: required_parameter / optional_parameter
            // have a `type` field (a `type_annotation`).
            "required_parameter" | "optional_parameter" => {
                if let Some(t) = n.child_by_field_name("type") {
                    collect_type_names(t, source, &mut inputs);
                }
            }
            // Return type annotation on functions / arrows / methods.
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "function_signature" => {
                if let Some(t) = n.child_by_field_name("return_type") {
                    collect_type_names(t, source, &mut outputs);
                }
            }
            _ => {}
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.node().id() == root_id {
                return (inputs, outputs);
            }
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return (inputs, outputs);
            }
        }
    }
}

/// Walk a type_annotation subtree and collect every type identifier
/// name encountered (both bare `type_identifier` and the name field of
/// `generic_type`, e.g. `Promise<User>` yields `Promise` and `User`).
fn collect_type_names(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut FxHashSet<String>,
) {
    let mut cursor = root.walk();
    let root_id = root.id();
    loop {
        let n = cursor.node();
        if n.kind() == "type_identifier"
            && let Ok(name) = std::str::from_utf8(&source[n.byte_range()])
        {
            out.insert(name.to_string());
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.node().id() == root_id {
                return;
            }
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // We fire once at the program root: collect input/output usage of
    // every type identifier and the set of exported type names, then
    // iterate declarations.
    let (inputs, outputs) = collect_type_positions(node, source);
    let exported = collect_exported_type_names(node, source);

    let mut cursor = node.walk();
    let root_id = node.id();
    loop {
        let n = cursor.node();
        match n.kind() {
            "interface_declaration" => {
                if let Some(name_node) = n.child_by_field_name("name")
                    && let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()])
                    && let Some(body) = n.child_by_field_name("body")
                    && type_qualifies(name, &inputs, &outputs, &exported)
                {
                    check_decl(n, name, body, source, ctx.path, diagnostics);
                }
            }
            "type_alias_declaration" => {
                if let Some(name_node) = n.child_by_field_name("name")
                    && let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()])
                    && let Some(value) = n.child_by_field_name("value")
                    && value.kind() == "object_type"
                    && type_qualifies(name, &inputs, &outputs, &exported)
                {
                    check_decl(n, name, value, source, ctx.path, diagnostics);
                }
            }
            _ => {}
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.node().id() == root_id {
                return;
            }
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_input_type_with_server_fields_when_used_as_param() {
        let d = run(
            "export interface CreateUserInput { id: string; name: string; createdAt: string }\n\
             function create(input: CreateUserInput) { return input; }",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("CreateUserInput"));
    }

    #[test]
    fn flags_bare_entity_used_as_both_input_and_output() {
        let d = run(
            "export interface User { id: string; name: string; createdAt: string }\n\
             function save(u: User): User { return u; }",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_type_alias_request_with_server_fields_when_used_as_param() {
        let d = run(
            "export type UpdateOrderRequest = { id: string; total: number; updatedAt: string };\n\
             function upd(r: UpdateOrderRequest) { return r; }",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_when_exported_via_separate_export_clause() {
        // `export { User }` after the declaration also puts the type on the
        // module's public surface.
        let d = run(
            "interface User { id: string; name: string; createdAt: string }\n\
             function save(u: User): User { return u; }\n\
             export { User };",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_response_type_with_server_fields() {
        // Response suffix used as return type only — that's fine.
        assert!(
            run(
                "export interface UserResponse { id: string; name: string; createdAt: string }\n\
             function get(): UserResponse { return {} as UserResponse; }",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_input_without_server_fields() {
        assert!(
            run(
                "export interface CreateUserInput { name: string; email: string }\n\
             function create(input: CreateUserInput) { return input; }",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_bare_entity_used_only_as_output() {
        // REVIEW regression: a "bare entity" type with server fields used
        // *only* in a return position should NOT be flagged — it is acting
        // purely as an output DTO.
        assert!(
            run(
                "export interface User { id: string; name: string; createdAt: string }\n\
             function getUser(): User { return {} as User; }",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unused_declaration() {
        // A standalone declaration with no input/output usage in the file
        // shouldn't be flagged — we can't prove it's misused.
        assert!(
            run("export interface User { id: string; name: string; createdAt: string }").is_empty()
        );
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
