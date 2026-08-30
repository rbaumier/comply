//! GeneratedShapeIndex — the object shapes a project's code generators own.
//!
//! Operator consequence: a rule asking "does a generated type already carry
//! these field names?" reads `ctx.project.generated_shape_index(globs)` instead
//! of resolving imports or re-reading codegen output once per linted file.
//!
//! How:
//! - Candidates are the linted TS/JS files, plus the `.d.ts` declaration files
//!   comply drops from the lint set when a glob names one — openapi-typescript
//!   writes `schema.d.ts` by default, and nothing else would reach it.
//! - A candidate is a generator's output when its path matches one of the
//!   configured globs or one of comply's path-level codegen signals
//!   (`*.gen.ts`, `*.generated.ts`, a `generated/` directory). Path only: the
//!   `@generated` banner scan needs the file's bytes, and opening every file in
//!   the project to look for one would cost more than the index saves.
//! - Each output is parsed once and every object type it declares is flattened,
//!   nesting included, so `Database.public.Tables.<table>.Row` and
//!   `components.schemas.<Name>` land next to a plain `export interface`.

use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::{PropertyKey, TSSignature, TSType};
use oxc_parser::Parser;
use rustc_hash::FxHashSet;

/// How deep the flattening descends into nested object types. Supabase's
/// `Database.public.Tables.<table>.Row` sits at depth 5; the cap is here only so
/// a pathological generated file cannot recurse without bound.
const MAX_NESTING: usize = 8;

/// One object type a generator declares, lifted out of the nesting it was
/// written in.
#[derive(Debug)]
pub struct GeneratedShape {
    /// The expression a use site writes to reach the shape: the exported root
    /// name followed by one indexed access per level of nesting, e.g.
    /// `components['schemas']['Agent']` or
    /// `Database['public']['Tables']['profiles']['Row']`.
    pub access: String,
    /// The generated file the shape was read from, relative to the project root
    /// when it sits under it. Display form — it exists to name the file in a
    /// diagnostic, not to be reopened.
    pub origin: String,
    /// Field names the shape carries. Private: the matching lives on the
    /// index, so no consumer has to reimplement what "covers" means.
    fields: FxHashSet<String>,
}

#[derive(Debug, Default)]
pub struct GeneratedShapeIndex {
    /// Source order within a file, files in sorted path order — so a shape that
    /// covers a given field set is always the same one across runs.
    shapes: Vec<GeneratedShape>,
}

impl GeneratedShapeIndex {
    /// Read every generated file among `candidates` and flatten its object
    /// types. `root` anchors the `origin` display paths; `None` leaves them
    /// absolute.
    pub fn build(candidates: &[PathBuf], globs: &[String], root: Option<&Path>) -> Self {
        let mut generated: Vec<&PathBuf> = candidates
            .iter()
            .filter(|path| is_generated_output(path, globs))
            .collect();
        // Two candidate sources (the linted set and the declaration-file walk)
        // can name the same file, and their order is the walk's, not the disk's.
        generated.sort_unstable();
        generated.dedup();

        let mut shapes = Vec::new();
        for path in generated {
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            let origin = display_path(path, root);
            collect_shapes(&source, path, &origin, &mut shapes);
        }
        Self { shapes }
    }

    /// True when no generated file was found. A project whose codegen output is
    /// out of reach has nothing to compare against, and its rules stay silent
    /// rather than warn about a missing configuration.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// The one generated shape carrying every name in `fields`, or `None`.
    ///
    /// A generated shape with extra fields still covers — a hand-written type
    /// copies part of a contract as readily as all of it. But a name set that
    /// several *different* contracts cover names none of them: `{ id, name }`
    /// sits on every table of a schema, so finding it there says nothing about
    /// where the hand-written type came from. Only an unambiguous cover is
    /// evidence. Supabase's `Row`, `Insert` and `Update` of one table carry the
    /// same field names, so they count as the one contract they are.
    pub fn covering(&self, fields: &FxHashSet<String>) -> Option<&GeneratedShape> {
        let mut covering = self.shapes.iter().filter(|shape| {
            // A shape smaller than the query cannot contain it — checked first
            // so the common miss costs one integer compare, not a set walk.
            shape.fields.len() >= fields.len()
                && fields.iter().all(|field| shape.fields.contains(field))
        });
        let first = covering.next()?;
        covering
            .all(|other| other.fields == first.fields)
            .then_some(first)
    }
}

/// True when `path` is a code generator's output: it matches one of the
/// configured globs, or one of the path-level codegen signals comply already
/// recognizes everywhere else.
///
/// A build/tooling config is never output, whatever it is named. The generator
/// and its configuration share a stem often enough that the two collide —
/// `openapi-ts.config.ts` sits under a `**/openapi*.ts` glob — and reading a
/// config's option types as a contract would put every project's `Options` type
/// up for redeclaration.
fn is_generated_output(path: &Path, globs: &[String]) -> bool {
    if crate::rules::path_utils::is_config_file(path) {
        return false;
    }
    crate::rules::path_utils::matches_any_glob(path, globs)
        || crate::rules::file_ctx::is_generated_path(path)
}

/// `path` relative to `root`, or the path itself when it does not sit under it.
///
/// Both sides are canonicalized before stripping: candidate paths arrive
/// canonical from the import index but raw from the declaration-file walk, and
/// on macOS a raw `/var/…` never strips against a canonical `/private/var/…`
/// root.
fn display_path(path: &Path, root: Option<&Path>) -> String {
    let Some(root) = root.and_then(|root| std::fs::canonicalize(root).ok()) else {
        return path.display().to_string();
    };
    let canonical = std::fs::canonicalize(path);
    let relative = canonical.as_deref().unwrap_or(path).strip_prefix(&root);
    relative.unwrap_or(path).display().to_string()
}

/// Parse `source` and append every object type it declares to `shapes`.
///
/// A parse failure yields nothing: a generated file comply cannot read is the
/// same as a project with no codegen, and a linter degrades rather than fails.
fn collect_shapes(source: &str, path: &Path, origin: &str, shapes: &mut Vec<GeneratedShape>) {
    let allocator = Allocator::default();
    let source_type = crate::oxc_helpers::source_type_for_path(path);
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked {
        return;
    }
    // Statement-level walk over the whole program, not just its top level: a
    // generator can wrap its output in `declare module "…" { … }`, and TS
    // forbids a type declaration anywhere else, so no shape is missed by never
    // descending into expressions.
    for_each_type_declaration(&parsed.program.body, &mut |name, members| {
        push_shape(name, members, origin, shapes, 0);
    });
}

/// Call `visit(name, members)` for every `type X = { … }` and `interface X { … }`
/// reachable from `statements`, descending through `export` and
/// `declare module` wrappers.
fn for_each_type_declaration<'a>(
    statements: &'a oxc_allocator::Vec<'a, oxc_ast::ast::Statement<'a>>,
    visit: &mut impl FnMut(&str, &'a oxc_allocator::Vec<'a, TSSignature<'a>>),
) {
    use oxc_ast::ast::{Declaration, Statement, TSModuleDeclarationBody};

    for statement in statements {
        if let Statement::TSModuleDeclaration(module) = statement {
            if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &module.body {
                for_each_type_declaration(&block.body, visit);
            }
            continue;
        }
        let declaration = match statement {
            Statement::ExportNamedDeclaration(export) => export.declaration.as_ref(),
            other => other.as_declaration(),
        };
        match declaration {
            Some(Declaration::TSInterfaceDeclaration(interface)) => {
                visit(interface.id.name.as_str(), &interface.body.body);
            }
            Some(Declaration::TSTypeAliasDeclaration(alias)) => {
                if let TSType::TSTypeLiteral(literal) = &alias.type_annotation {
                    visit(alias.id.name.as_str(), &literal.members);
                }
            }
            _ => {}
        }
    }
}

/// Record the object type `access` names, then recurse into every field whose
/// own type is an object — that recursion is what reaches the row types buried
/// under `Database.public.Tables.<table>` and the schemas under
/// `components.schemas`.
fn push_shape(
    access: &str,
    members: &oxc_allocator::Vec<'_, TSSignature<'_>>,
    origin: &str,
    shapes: &mut Vec<GeneratedShape>,
    depth: usize,
) {
    if depth > MAX_NESTING {
        return;
    }
    let fields: FxHashSet<String> = members
        .iter()
        .filter_map(property_name)
        .map(str::to_string)
        .collect();
    if !fields.is_empty() {
        shapes.push(GeneratedShape {
            access: access.to_string(),
            origin: origin.to_string(),
            fields,
        });
    }
    for member in members {
        let TSSignature::TSPropertySignature(property) = member else {
            continue;
        };
        let (Some(name), Some(annotation)) = (property_name(member), &property.type_annotation)
        else {
            continue;
        };
        let TSType::TSTypeLiteral(literal) = &annotation.type_annotation else {
            continue;
        };
        push_shape(
            &format!("{access}['{name}']"),
            &literal.members,
            origin,
            shapes,
            depth + 1,
        );
    }
}

/// The field name a signature declares, or `None` for anything that is not a
/// plain named property — an index signature, a method, a computed key.
fn property_name<'a>(member: &'a TSSignature<'a>) -> Option<&'a str> {
    let TSSignature::TSPropertySignature(property) = member else {
        return None;
    };
    if property.computed {
        return None;
    }
    match &property.key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.as_str()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an index over one file written to a temp dir, the way a project's
    /// codegen output reaches it.
    fn index_of(name: &str, source: &str) -> (tempfile::TempDir, GeneratedShapeIndex) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(name);
        std::fs::write(&path, source).expect("write fixture");
        let index = GeneratedShapeIndex::build(&[path], &["**/*.ts".to_string()], Some(dir.path()));
        (dir, index)
    }

    fn field_set(names: &[&str]) -> FxHashSet<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    /// Supabase emits `Row`, `Insert` and `Update` with the same column names,
    /// so the three collapse to the one contract they describe and the `Row`
    /// — declared first — is the one named back.
    #[test]
    fn flattens_a_supabase_row() {
        let (_dir, index) = index_of(
            "database.types.ts",
            "export type Database = {\n\
               public: {\n\
                 Tables: {\n\
                   profiles: {\n\
                     Row: { id: string; email: string; created_at: string }\n\
                     Insert: { id?: string; email: string; created_at?: string }\n\
                   }\n\
                 }\n\
               }\n\
             }",
        );
        let shape = index
            .covering(&field_set(&["id", "email"]))
            .expect("the row covers both fields");
        assert_eq!(shape.access, "Database['public']['Tables']['profiles']['Row']");
        assert_eq!(shape.origin, "database.types.ts");
    }

    #[test]
    fn flattens_an_openapi_schema() {
        let (_dir, index) = index_of(
            "openapi.ts",
            "export interface components {\n\
               schemas: {\n\
                 Agent: { id: string; name: string }\n\
               }\n\
             }",
        );
        let shape = index
            .covering(&field_set(&["id", "name"]))
            .expect("the schema covers both fields");
        assert_eq!(shape.access, "components['schemas']['Agent']");
    }

    /// Two tables both carrying `{ id, name }` make the match point at neither.
    #[test]
    fn does_not_cover_when_two_contracts_qualify() {
        let (_dir, index) = index_of(
            "database.types.ts",
            "export type Database = {\n\
               public: {\n\
                 Tables: {\n\
                   teams: { Row: { id: string; name: string; slug: string } }\n\
                   products: { Row: { id: string; name: string; price: number } }\n\
                 }\n\
               }\n\
             }",
        );
        assert!(index.covering(&field_set(&["id", "name"])).is_none());
        assert!(index.covering(&field_set(&["id", "slug"])).is_some());
    }

    /// A field the generated type does not carry means the hand-written type is
    /// not a copy of it, so nothing covers it.
    #[test]
    fn does_not_cover_a_superset() {
        let (_dir, index) = index_of("api.generated.ts", "export interface User { id: string }");
        assert!(index.covering(&field_set(&["id", "nickname"])).is_none());
    }

    #[test]
    fn ignores_a_file_no_glob_and_no_signal_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("domain.ts");
        std::fs::write(&path, "export interface User { id: string; email: string }").unwrap();
        let index = GeneratedShapeIndex::build(
            &[path],
            &["**/database.types.ts".to_string()],
            Some(dir.path()),
        );
        assert!(index.is_empty());
    }

    /// `openapi-ts.config.ts` configures the generator; it is not its output,
    /// even though a `**/openapi*.ts` glob covers the name.
    #[test]
    fn ignores_a_generator_config_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("openapi-ts.config.ts");
        std::fs::write(&path, "export interface Options { input: string; output: string }").unwrap();
        let index =
            GeneratedShapeIndex::build(&[path], &["**/openapi*.ts".to_string()], Some(dir.path()));
        assert!(index.is_empty());
    }

    /// `*.generated.ts` is one of comply's path-level codegen signals, so it is
    /// indexed even when the configured glob list is empty.
    #[test]
    fn indexes_a_codegen_filename_without_a_glob() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("api.generated.ts");
        std::fs::write(&path, "export interface User { id: string; email: string }").unwrap();
        let index = GeneratedShapeIndex::build(&[path], &[], Some(dir.path()));
        assert!(index.covering(&field_set(&["id", "email"])).is_some());
    }
}
