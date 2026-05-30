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
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
