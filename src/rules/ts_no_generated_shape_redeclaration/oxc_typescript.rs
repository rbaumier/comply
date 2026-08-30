//! ts-no-generated-shape-redeclaration oxc backend — compare every
//! hand-written object type in the file against the shapes the project's
//! generators own.
//!
//! The comparison is on field *names* only. Types drifting apart is the failure
//! mode the rule exists to catch, so requiring them to match would blind it to
//! exactly the copies that already rotted.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::path_utils::matches_any_glob;
use oxc_ast::ast::{PropertyKey, TSSignature, TSType};
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// Type-level markers of behaviour rather than data. A shape carrying one is a
/// component's props or a handler bag — it shares field names with a generated
/// row by coincidence, never by copy.
const BEHAVIOUR_MARKERS: [&str; 3] = ["=>", "ReactNode", "JSX"];

pub struct Check;

/// A hand-written object type that could be a copy: the name to report, where
/// to point, and the field names it declares.
struct Declared<'a> {
    name: &'a str,
    offset: usize,
    fields: FxHashSet<String>,
}

impl OxcCheck for Check {
    /// Empty: the rule reads its configuration and the project index once per
    /// file, then walks the nodes itself. Per-node dispatch would re-read both
    /// on every type declaration.
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let globs = ctx
            .config
            .string_list(super::META.id, "generated_globs", ctx.lang);
        // A project whose generators are out of reach has no contract to
        // redeclare. Checked before anything per-file so that project pays one
        // lock-free read per file and nothing else.
        let index = ctx.project.generated_shape_index(&globs);
        if index.is_empty() {
            return Vec::new();
        }
        // A declaration inside the generator's own output *is* the contract.
        // `is_generated` covers the `@generated` banner and comply's path
        // signals; the globs cover the outputs that carry neither, such as a
        // Supabase `database.types.ts`.
        if ctx.file.is_generated || matches_any_glob(ctx.path, &globs) {
            return Vec::new();
        }

        let min_fields = ctx.config.threshold(super::META.id, "min_fields", ctx.lang);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let Some(declared) = declared_shape(node.kind(), ctx.source, min_fields) else {
                continue;
            };
            // A module-private shape *is* the use-site narrowing the rule asks
            // for: it names a projection for the one file that consumes it, and
            // no other file can drift from it. Only a shape that leaves its
            // module carries the copy across the codebase.
            if !matches!(
                semantic.nodes().parent_kind(node.id()),
                AstKind::ExportNamedDeclaration(_)
            ) {
                continue;
            }
            let Some(shape) = index.covering(&declared.fields) else {
                continue;
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, declared.offset);
            let name = declared.name;
            let access = &shape.access;
            let origin = &shape.origin;
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{name}` redeclares fields of generated `{access}` ({origin}) — read \
                     the generated type at the use site and narrow it there. An \
                     intermediate alias is a second copy of the same contract."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

/// The hand-written object type `kind` declares, or `None` when it declares
/// something a generated type cannot stand in for.
///
/// A generic declaration is out: what `Envelope<T>` describes depends on the
/// argument. An `interface X extends Y` is out too — it composes a shape rather
/// than restating one.
fn declared_shape<'a>(
    kind: AstKind<'a>,
    source: &str,
    min_fields: usize,
) -> Option<Declared<'a>> {
    let (name, offset, members) = match kind {
        AstKind::TSTypeAliasDeclaration(alias) if alias.type_parameters.is_none() => {
            let TSType::TSTypeLiteral(literal) = &alias.type_annotation else {
                return None;
            };
            (
                alias.id.name.as_str(),
                alias.id.span.start as usize,
                &literal.members,
            )
        }
        AstKind::TSInterfaceDeclaration(interface)
            if interface.type_parameters.is_none() && interface.extends.is_empty() =>
        {
            (
                interface.id.name.as_str(),
                interface.id.span.start as usize,
                &interface.body.body,
            )
        }
        _ => return None,
    };
    let fields = copyable_fields(members, source, min_fields)?;
    Some(Declared {
        name,
        offset,
        fields,
    })
}

/// The field names of an object type that could have been copied from generated
/// output, or `None` when the type is not a plain data shape.
///
/// Every member has to be a plainly named property: a method, an index
/// signature or a computed key describes behaviour or an open dictionary, and a
/// generated row is neither. One field typed as a callback or as JSX rules the
/// whole type out the same way — that is a component's props, and its `id` /
/// `label` / `value` names collide with a generated row by coincidence.
fn copyable_fields(
    members: &oxc_allocator::Vec<'_, TSSignature<'_>>,
    source: &str,
    min_fields: usize,
) -> Option<FxHashSet<String>> {
    let mut fields = FxHashSet::default();
    for member in members {
        let TSSignature::TSPropertySignature(property) = member else {
            return None;
        };
        if property.computed {
            return None;
        }
        let name = match &property.key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name.as_str(),
            PropertyKey::StringLiteral(literal) => literal.value.as_str(),
            _ => return None,
        };
        if let Some(annotation) = &property.type_annotation {
            let text = &source[annotation.span.start as usize..annotation.span.end as usize];
            if BEHAVIOUR_MARKERS.iter().any(|marker| text.contains(marker)) {
                return None;
            }
        }
        fields.insert(name.to_string());
    }
    // Below the threshold two unrelated types share their field names often
    // enough that a match says nothing about where they came from.
    (fields.len() >= min_fields).then_some(fields)
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
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;

    /// A Supabase `database.types.ts`, trimmed to one table but keeping the
    /// nesting and the `Row`/`Insert`/`Update` triple the generator emits.
    const SUPABASE: &str = r#"
export type Json = string | number | boolean | null | { [key: string]: Json } | Json[];

export type Database = {
  public: {
    Tables: {
      profiles: {
        Row: {
          id: string
          email: string
          full_name: string | null
          created_at: string
        }
        Insert: {
          id?: string
          email: string
          full_name?: string | null
          created_at?: string
        }
        Update: {
          id?: string
          email?: string
          full_name?: string | null
          created_at?: string
        }
        Relationships: []
      }
    }
    Views: { [_ in never]: never }
    Enums: { [_ in never]: never }
  }
}

export type Tables<T extends keyof Database["public"]["Tables"]> =
  Database["public"]["Tables"][T]["Row"]
"#;

    /// An openapi-typescript `openapi.ts`, trimmed to one schema.
    const OPENAPI: &str = r#"
export interface paths {
  "/agents": {
    get: { responses: { 200: { content: { "application/json": components["schemas"]["Agent"] } } } }
  }
}

export interface components {
  schemas: {
    Agent: {
      id: string
      name: string
      status: "idle" | "busy"
      last_seen_at: string
    }
  }
  responses: never
  parameters: never
}
"#;

    /// Run the rule on `source` written at `rel`, inside a project that also
    /// contains `generated`. Mirrors production: the project index is built
    /// from the real file set, the file context from the real path and source,
    /// and the rule only runs when its directory gate lets it.
    fn run_in_project(
        generated: &[(&str, &str)],
        rel: &str,
        source: &str,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut files = Vec::new();
        for (name, contents) in generated.iter().copied().chain([(rel, source)]) {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
            std::fs::write(&path, contents).expect("write fixture");
            if let Some(language) = Language::from_path(&path) {
                files.push(SourceFile { path, language });
            }
        }
        let refs: Vec<&SourceFile> = files.iter().collect();
        let mut project = ProjectCtx::for_test_with_files(&refs);
        project.project_root = Some(dir.path().to_path_buf());

        let path = dir.path().join(rel);
        let language = Language::from_path(&path).unwrap_or(Language::TypeScript);
        let file = FileCtx::build(&path, source, language, &project);
        if !super::super::META.applies_to_file(&file) {
            return Vec::new();
        }
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &path, &project, &file)
    }

    fn run_with_supabase(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        run_in_project(&[("src/database.types.ts", SUPABASE)], "src/user.ts", source)
    }

    #[test]
    fn flags_interface_copying_a_row_field_for_field() {
        let source = "export interface Profile {\n\
                        id: string;\n\
                        email: string;\n\
                        full_name: string | null;\n\
                        created_at: string;\n\
                      }";
        assert_eq!(run_with_supabase(source).len(), 1);
    }

    /// A strict subset of the row's fields is the copy that hurts most: it
    /// keeps compiling after the columns it left out change.
    #[test]
    fn flags_interface_copying_part_of_a_row() {
        let source = "export interface ProfileSummary {\n\
                        id: string;\n\
                        email: string;\n\
                      }";
        assert_eq!(run_with_supabase(source).len(), 1);
    }

    #[test]
    fn flags_type_literal_copying_an_openapi_schema() {
        let source = "export type Agent = { id: string; name: string; status: string };";
        let diagnostics = run_in_project(&[("src/openapi.ts", OPENAPI)], "src/agent.ts", source);
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn message_names_the_type_the_generated_access_and_the_file() {
        let source = "export type Agent = { id: string; name: string };";
        let diagnostics = run_in_project(&[("src/openapi.ts", OPENAPI)], "src/agent.ts", source);
        let message = &diagnostics[0].message;
        assert!(message.contains("`Agent`"), "{message}");
        assert!(
            message.contains("components['schemas']['Agent']"),
            "{message}"
        );
        assert!(message.contains("src/openapi.ts"), "{message}");
    }

    /// One field the generated row does not carry, and the type is a shape of
    /// its own rather than a copy.
    #[test]
    fn ignores_a_type_with_a_field_the_generated_type_lacks() {
        let source = "export interface ProfileRow {\n\
                        id: string;\n\
                        email: string;\n\
                        is_selected: boolean;\n\
                      }";
        assert!(run_with_supabase(source).is_empty());
    }

    #[test]
    fn ignores_a_single_field_type() {
        let source = "export type ProfileId = { id: string };";
        assert!(run_with_supabase(source).is_empty());
    }

    #[test]
    fn ignores_component_props_with_a_callback_field() {
        let source = "export type ProfileCardProps = {\n\
                        id: string;\n\
                        email: string;\n\
                        onSelect: (id: string) => void;\n\
                      };";
        assert!(run_with_supabase(source).is_empty());
    }

    #[test]
    fn ignores_component_props_with_a_react_node_field() {
        let source = "export type ProfileCardProps = {\n\
                        id: string;\n\
                        email: string;\n\
                        badge: ReactNode;\n\
                      };";
        assert!(run_with_supabase(source).is_empty());
    }

    /// The declarations inside the generated file are the contract itself.
    #[test]
    fn ignores_declarations_inside_the_generated_file() {
        let diagnostics = run_in_project(
            &[("src/openapi.ts", OPENAPI)],
            "src/database.types.ts",
            SUPABASE,
        );
        assert!(diagnostics.is_empty());
    }

    /// No generated file in the project: nothing to compare against, and no
    /// complaint about the missing configuration either.
    #[test]
    fn stays_silent_in_a_project_with_no_generated_file() {
        let source = "export interface Profile { id: string; email: string };";
        let diagnostics = run_in_project(&[("src/domain.ts", "export const x = 1;")], "src/user.ts", source);
        assert!(diagnostics.is_empty());
    }

    /// What `Row<T>` describes depends on the argument, so it is not a second
    /// declaration of any one generated shape.
    #[test]
    fn ignores_a_generic_type() {
        let source = "export type Envelope<T> = { id: string; email: string };";
        assert!(run_with_supabase(source).is_empty());
    }

    #[test]
    fn ignores_an_interface_extending_another_type() {
        let source = "export interface Profile extends Base { id: string; email: string }";
        assert!(run_with_supabase(source).is_empty());
    }

    #[test]
    fn ignores_a_type_carrying_a_method() {
        let source = "export interface Profile {\n\
                        id: string;\n\
                        email: string;\n\
                        refresh(): void;\n\
                      }";
        assert!(run_with_supabase(source).is_empty());
    }

    #[test]
    fn ignores_an_index_signature() {
        let source = "export interface ProfileMap {\n\
                        id: string;\n\
                        email: string;\n\
                        [key: string]: unknown;\n\
                      }";
        assert!(run_with_supabase(source).is_empty());
    }

    /// A shape no other file can see is already narrowed at its use site.
    #[test]
    fn ignores_a_module_private_type() {
        let source = "type Profile = { id: string; email: string };\n\
                      export const read = (p: Profile) => p.id;";
        assert!(run_with_supabase(source).is_empty());
    }

    /// `{ id, name }` sits on both tables of this fixture, so it identifies
    /// neither: the hand-written type shares those names with the schema the
    /// way every entity type does.
    #[test]
    fn ignores_a_shape_two_generated_tables_both_cover() {
        const TWO_TABLES: &str = r#"
export type Database = {
  public: {
    Tables: {
      teams: { Row: { id: string; name: string; slug: string } }
      products: { Row: { id: string; name: string; price: number } }
    }
  }
}
"#;
        let source = "export type Entity = { id: string; name: string };";
        let diagnostics = run_in_project(
            &[("src/database.types.ts", TWO_TABLES)],
            "src/entity.ts",
            source,
        );
        assert!(diagnostics.is_empty());
    }

    /// Test files restate fixtures on purpose; the rule's directory gate keeps
    /// it out of them.
    #[test]
    fn ignores_a_test_file() {
        let source = "export interface Profile { id: string; email: string }";
        let diagnostics = run_in_project(
            &[("src/database.types.ts", SUPABASE)],
            "src/__tests__/user.ts",
            source,
        );
        assert!(diagnostics.is_empty());
    }
}
