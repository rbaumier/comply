//! ts-no-generated-type-alias oxc backend — flag an alias whose right-hand side
//! is a generated type, either named as-is or narrowed by one utility type.
//!
//! A module counts as generated when its import specifier matches one of the
//! `generated_modules` globs. The list is configuration because only the project
//! knows where its codegen writes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{IdentifierReference, TSType, TSTypeName};
use std::sync::Arc;

/// Utility types that keep naming the generated shape: the result is the
/// contract minus fields or minus required-ness, never a different type. A
/// utility outside this list (`Record`, a project generic) builds something the
/// generator does not own.
const DERIVING_UTILITIES: [&str; 3] = ["Pick", "Omit", "Partial"];

pub struct Check;

/// The generated type an alias copies: the identifier that must resolve to an
/// import, and the utility applied to it when there is one.
struct Derivation<'a> {
    root: &'a IdentifierReference<'a>,
    utility: Option<&'a str>,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    /// The right-hand side has to name a type that came from another module, so
    /// a file with no import declaration holds nothing to report.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["import"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
            return;
        };
        // What `Envelope<T>` stands for depends on the argument, so a generic
        // alias is not a second name for the type it mentions.
        if alias.type_parameters.is_some() {
            return;
        }
        let Some(derivation) = classify(&alias.type_annotation) else {
            return;
        };
        let Some(source) = import_source_of(derivation.root, semantic) else {
            return;
        };
        let patterns = ctx
            .config
            .string_list(super::META.id, "generated_modules", ctx.lang);
        if !crate::rules::path_utils::matches_any_glob(source, &patterns) {
            return;
        }

        let name = alias.id.name.as_str();
        let subject = match derivation.utility {
            Some(utility) => {
                format!("`{name}` derives from the generated type in `{source}` with `{utility}`")
            }
            None => format!("`{name}` renames the generated type from `{source}`"),
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, alias.id.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{subject} — the copy hides the contract and the next regeneration \
                 moves one side only. Read the generated type directly at every use \
                 site; when the shape you need is missing, add it where the generator \
                 reads from."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// The generated type a right-hand side copies, when it copies one.
///
/// Only two shapes qualify: a bare type reference (`Q.Y`, `Y`) and one of
/// [`DERIVING_UTILITIES`] applied to a bare type reference. Everything else —
/// a union, an intersection, a mapped type, an array, a utility over an already
/// parameterized type — builds a type the generator does not describe.
fn classify<'a>(annotation: &'a TSType<'a>) -> Option<Derivation<'a>> {
    let TSType::TSTypeReference(reference) = annotation else {
        return None;
    };
    let Some(arguments) = &reference.type_arguments else {
        return leftmost_identifier(&reference.type_name).map(|root| Derivation {
            root,
            utility: None,
        });
    };
    let TSTypeName::IdentifierReference(utility) = &reference.type_name else {
        return None;
    };
    let utility = utility.name.as_str();
    if !DERIVING_UTILITIES.contains(&utility) {
        return None;
    }
    let TSType::TSTypeReference(target) = arguments.params.first()? else {
        return None;
    };
    if target.type_arguments.is_some() {
        return None;
    }
    leftmost_identifier(&target.type_name).map(|root| Derivation {
        root,
        utility: Some(utility),
    })
}

/// The identifier a type name starts with — `Schemas` in `Schemas.Agent.Row`,
/// the name itself when the type name is unqualified. That leftmost identifier
/// is the only part of a qualified name a module can bind.
fn leftmost_identifier<'a>(name: &'a TSTypeName<'a>) -> Option<&'a IdentifierReference<'a>> {
    let mut current = name;
    loop {
        match current {
            TSTypeName::IdentifierReference(identifier) => return Some(identifier),
            TSTypeName::QualifiedName(qualified) => current = &qualified.left,
            _ => return None,
        }
    }
}

/// The module specifier `identifier` was imported from, or `None` when it
/// resolves to anything else — a local declaration, a shadowing binding, an
/// unresolved global. Resolution goes through the symbol, so a locally declared
/// type that shares a name with an import is never mistaken for it.
fn import_source_of<'a>(
    identifier: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    let scoping = semantic.scoping();
    let reference_id = identifier.reference_id.get()?;
    let symbol_id = scoping.get_reference(reference_id).symbol_id()?;
    let declaration_id = scoping.symbol_declaration(symbol_id);
    let nodes = semantic.nodes();
    if !matches!(
        nodes.kind(declaration_id),
        AstKind::ImportSpecifier(_)
            | AstKind::ImportDefaultSpecifier(_)
            | AstKind::ImportNamespaceSpecifier(_)
    ) {
        return None;
    }
    nodes.ancestors(declaration_id).find_map(|node| match node.kind() {
        AstKind::ImportDeclaration(declaration) => Some(declaration.source.value.as_str()),
        _ => None,
    })
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

    /// Every fixture imports from `#/api/generated/types.gen`, which the default
    /// `generated_modules` globs cover.
    const GENERATED: &str = "#/api/generated/types.gen";

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_qualified_alias() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type ChannelType = Schemas.ChannelType;"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_alias_of_named_import() {
        let src = format!(
            "import type {{ ChannelType }} from \"{GENERATED}\";\n\
             type Channel = ChannelType;"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_exported_alias() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             export type Agent = Schemas.AgentResponse;"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_pick_of_generated() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Row = Pick<Schemas.AgentResponse, \"id\" | \"name\">;"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_omit_of_generated() {
        let src = format!(
            "import type {{ AgentResponse }} from \"{GENERATED}\";\n\
             type Draft = Omit<AgentResponse, \"id\">;"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_partial_of_generated() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Patch = Partial<Schemas.AgentResponse>;"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_alias_of_default_import() {
        let src = format!(
            "import type AgentResponse from \"{GENERATED}\";\n\
             type Agent = AgentResponse;"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn message_names_the_alias_and_the_module() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type ChannelType = Schemas.ChannelType;"
        );
        let diagnostics = run(&src);
        assert!(diagnostics[0].message.contains("`ChannelType`"));
        assert!(diagnostics[0].message.contains(GENERATED));
    }

    #[test]
    fn ignores_alias_of_local_type() {
        let src = "import { noop } from \"./util\";\n\
                   type Base = { id: string };\n\
                   type Alias = Base;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_pick_of_local_type() {
        let src = "import { noop } from \"./util\";\n\
                   type Base = { id: string; name: string };\n\
                   type Row = Pick<Base, \"id\">;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_alias_of_ungenerated_import() {
        let src = "import type { Agent } from \"#/domain/agent\";\ntype Alias = Agent;";
        assert!(run(src).is_empty());
    }

    /// A local declaration shadowing the imported name is a different type, so
    /// symbol resolution — not the name — decides.
    #[test]
    fn ignores_local_type_shadowing_a_generated_name() {
        let src = format!(
            "import {{ noop }} from \"{GENERATED}\";\n\
             type AgentResponse = {{ id: string }};\n\
             type Alias = AgentResponse;"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_union_with_generated_member() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Loadable = Schemas.AgentResponse | null;"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_intersection_with_generated_member() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Decorated = Schemas.AgentResponse & {{ selected: boolean }};"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_mapped_type_over_generated() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Flags = {{ [K in keyof Schemas.AgentResponse]: boolean }};"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_array_of_generated() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Rows = Schemas.AgentResponse[];"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_indexed_access_of_generated() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Id = Schemas.AgentResponse[\"id\"];"
        );
        assert!(run(&src).is_empty());
    }

    /// `Record` builds a type the generator never described, even when a
    /// generated type supplies one of its arguments.
    #[test]
    fn ignores_non_deriving_utility() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type ById = Record<string, Schemas.AgentResponse>;"
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_generic_alias_over_generated() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Envelope<T> = Schemas.AgentResponse;"
        );
        assert!(run(&src).is_empty());
    }

    /// A re-export declares no alias: the name that leaves the module is the
    /// generated one.
    #[test]
    fn ignores_type_re_export() {
        let src = format!("export type {{ AgentResponse }} from \"{GENERATED}\";");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_namespace_re_export() {
        let src = format!("export type * as Schemas from \"{GENERATED}\";");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn flags_under_a_generated_directory_without_extension() {
        let src = "import type { AgentResponse } from \"@/api/generated\";\n\
                   type Alias = AgentResponse;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_under_a_relative_generated_directory() {
        let src = "import type { AgentResponse } from \"./generated/types\";\n\
                   type Alias = AgentResponse;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_under_a_double_underscore_generated_directory() {
        let src = "import type { AgentResponse } from \"../__generated__/graphql\";\n\
                   type Alias = AgentResponse;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_module_named_after_a_generated_prefix() {
        let src = "import type { Report } from \"#/lib/generator/report\";\ntype Alias = Report;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_in_tsx() {
        let src = format!(
            "import type * as Schemas from \"{GENERATED}\";\n\
             type Agent = Schemas.AgentResponse;"
        );
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, &src, "t.tsx").len(),
            1
        );
    }

    /// An empty `generated_modules` list turns the rule off: the project
    /// declares no codegen output, so no alias can copy one.
    #[test]
    fn empty_pattern_list_matches_nothing() {
        assert!(!crate::rules::path_utils::matches_any_glob(
            "#/api/generated/types.gen",
            &[]
        ));
    }
}
