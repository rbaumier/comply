use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_semantic::SymbolId;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.config.bool_flag("prefer-export-from", "allow_import_then_reexport", ctx.lang) {
            return Vec::new();
        }
        let program = semantic.nodes().program();
        let mut diagnostics = Vec::new();

        let scoping = semantic.scoping();
        let nodes = semantic.nodes();

        // Phase 1: collect named imports as `local_name -> (module_specifier, symbol_id)`.
        let mut imports: HashMap<&str, (&str, Option<SymbolId>)> = HashMap::new();
        for stmt in &program.body {
            let Statement::ImportDeclaration(import) = stmt else {
                continue;
            };
            let Some(ref specifiers) = import.specifiers else {
                continue;
            };
            let specifier = import.source.value.as_str();
            for spec in specifiers {
                let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                    continue;
                };
                let local_name = named.local.name.as_str();
                let symbol_id = named.local.symbol_id.get();
                imports.insert(local_name, (specifier, symbol_id));
            }
        }

        if imports.is_empty() {
            return diagnostics;
        }

        // Phase 2: find `export { name }` statements (without `from`).
        for stmt in &program.body {
            let Statement::ExportNamedDeclaration(export) = stmt else {
                continue;
            };
            // Skip re-export-from forms — they already use the preferred shape.
            if export.source.is_some() {
                continue;
            }
            // Only look at bare `export { ... }` (no declaration).
            if export.declaration.is_some() {
                continue;
            }
            for spec in &export.specifiers {
                let local_name = spec.local.name().as_str();
                if let Some((module_specifier, sym_id)) = imports.get(local_name) {
                    // Skip if the symbol is also used locally — converting to a
                    // re-export would remove the local binding.
                    if let Some(symbol_id) = sym_id {
                        let has_local_usage =
                            scoping.get_resolved_references(*symbol_id).any(|reference| {
                                !nodes.ancestor_kinds(reference.node_id()).any(|k| {
                                    matches!(k, AstKind::ExportNamedDeclaration(_))
                                })
                            });
                        if has_local_usage {
                            continue;
                        }
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, spec.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Use `export {{ {local_name} }} from '{module_specifier}'` instead of \
                             importing then re-exporting `{local_name}`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::rules::backend::CheckCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_span::SourceType;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_with_flag_enabled(source: &str) -> Vec<Diagnostic> {
        let tmp = TempDir::new().expect("tempdir");
        fs::write(
            tmp.path().join("comply.toml"),
            "[rules.prefer-export-from]\nallow_import_then_reexport = true\n",
        )
        .expect("write cfg");
        let config = Config::load_from(tmp.path()).expect("load cfg");
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
        let semantic = oxc_semantic::SemanticBuilder::new().build(&parse_ret.program).semantic;
        let path = Path::new("t.ts");
        let ctx = CheckCtx {
            path,
            path_arc: std::sync::Arc::from(path),
            source,
            config: &config,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: crate::files::Language::TypeScript,
        };
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn flags_import_then_reexport() {
        let d = run("import { foo } from './mod';\nexport { foo };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("export { foo } from './mod'"));
    }

    #[test]
    fn allows_direct_export_from() {
        assert!(run("export { foo } from './mod';").is_empty());
    }

    #[test]
    fn allows_export_of_local() {
        assert!(run("const bar = 1;\nexport { bar };").is_empty());
    }

    #[test]
    fn no_fp_when_import_used_locally_and_exported() {
        // Symbol imported, used locally, and exported — cannot be converted to re-export.
        let src = "import { GammeSchema } from './gamme-schema';\nconst x = GammeSchema.parse({});\nexport { GammeSchema };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_import_aliased_used_locally_and_exported() {
        let src = "import { foo as bar } from './m';\nconsole.log(bar);\nexport { bar };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_allow_import_then_reexport_is_true() {
        // Regression test for issue #575: projects that ban `export { x } from`
        // can set `allow_import_then_reexport = true` to suppress the rule.
        let src = "import { ForbiddenError } from './forbidden-error';\nexport { ForbiddenError };";
        assert!(run_with_flag_enabled(src).is_empty());
    }
}
