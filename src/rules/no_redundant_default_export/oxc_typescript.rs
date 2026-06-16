//! no-redundant-default-export oxc backend.
//!
//! Collects, per module, the set of binding symbols that are named-exported and
//! the single default-exported symbol (if it is a bare identifier). When the
//! default symbol is also named-exported, the default export is redundant.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, ExportSpecifier, ModuleExportName,
    Statement,
};
use oxc_semantic::{Semantic, SymbolId};
use oxc_span::Span;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["export default", "as default"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut named: FxHashSet<SymbolId> = FxHashSet::default();
        let mut default_export: Option<(SymbolId, Span)> = None;

        for stmt in &semantic.nodes().program().body {
            match stmt {
                Statement::ExportNamedDeclaration(export) => {
                    // `export … from "…"` re-exports another module's binding,
                    // not a local one — out of scope.
                    if export.source.is_some() {
                        continue;
                    }
                    if let Some(decl) = &export.declaration {
                        collect_declared_symbols(decl, &mut named);
                    }
                    for spec in &export.specifiers {
                        collect_specifier(spec, semantic, &mut named, &mut default_export);
                    }
                }
                Statement::ExportDefaultDeclaration(export) => {
                    // A default export of an inline function/class declaration
                    // binds a fresh name; an anonymous expression binds none.
                    // Only `export default <identifier>` references an existing
                    // binding that a named export could duplicate.
                    if let ExportDefaultDeclarationKind::Identifier(reference) = &export.declaration
                        && let Some(symbol) = resolve_reference(reference, semantic)
                    {
                        default_export = Some((symbol, reference.span));
                    }
                }
                _ => {}
            }
        }

        let Some((symbol, span)) = default_export else {
            return Vec::new();
        };
        if !named.contains(&symbol) {
            return Vec::new();
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Default export references the same symbol as a named export — \
                      remove either the default or the named export."
                .into(),
            severity: super::META.severity,
            span: None,
        }]
    }
}

/// Add the binding symbols introduced by `export const/function/class …`.
fn collect_declared_symbols(decl: &Declaration, named: &mut FxHashSet<SymbolId>) {
    match decl {
        Declaration::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                if let BindingPattern::BindingIdentifier(id) = &declarator.id
                    && let Some(symbol) = id.symbol_id.get()
                {
                    named.insert(symbol);
                }
            }
        }
        Declaration::FunctionDeclaration(func) => {
            if let Some(id) = &func.id
                && let Some(symbol) = id.symbol_id.get()
            {
                named.insert(symbol);
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id
                && let Some(symbol) = id.symbol_id.get()
            {
                named.insert(symbol);
            }
        }
        _ => {}
    }
}

/// Handle one `export { local as exported }` specifier. `as default` marks the
/// default export; anything else is a named export of `local`'s binding.
fn collect_specifier(
    spec: &ExportSpecifier,
    semantic: &Semantic,
    named: &mut FxHashSet<SymbolId>,
    default_export: &mut Option<(SymbolId, Span)>,
) {
    let ModuleExportName::IdentifierReference(local) = &spec.local else {
        return;
    };
    let Some(symbol) = resolve_reference(local, semantic) else {
        return;
    };
    if exported_name_is_default(&spec.exported) {
        *default_export = Some((symbol, local.span));
    } else {
        named.insert(symbol);
    }
}

fn exported_name_is_default(name: &ModuleExportName) -> bool {
    match name {
        ModuleExportName::IdentifierName(id) => id.name == "default",
        ModuleExportName::IdentifierReference(id) => id.name == "default",
        ModuleExportName::StringLiteral(lit) => lit.value == "default",
    }
}

fn resolve_reference(
    reference: &oxc_ast::ast::IdentifierReference,
    semantic: &Semantic,
) -> Option<SymbolId> {
    let ref_id = reference.reference_id.get()?;
    semantic.scoping().get_reference(ref_id).symbol_id()
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- Biome invalid fixtures (must fire) ---

    #[test]
    fn fires_export_list_biome_export_list() {
        // const foo = 1; export { foo }; export default foo;
        let d = run_on("const foo = 1;\nexport { foo };\nexport default foo;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn fires_function_declaration_biome_function_declaration() {
        // export function foo() {} export default foo;
        let d = run_on("export function foo() {}\nexport default foo;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn fires_class_declaration_biome_class_declaration() {
        // export class MyClass {} export default MyClass;
        let d = run_on("export class MyClass {}\nexport default MyClass;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn fires_alias_named_export_biome_alias_named_export() {
        // const foo = 1; export { foo as bar }; export default foo;
        // The named export is aliased, but its local binding `foo` still
        // matches the default — redundant.
        let d = run_on("const foo = 1;\nexport { foo as bar };\nexport default foo;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn fires_import_binding_biome_import_binding() {
        // import foo from "./other"; export { foo }; export default foo;
        let d = run_on("import foo from \"./other\";\nexport { foo };\nexport default foo;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn fires_named_default_specifier_biome_named_default() {
        // const foo = 1; export { foo as default }; export { foo };
        let d = run_on("const foo = 1;\nexport { foo as default };\nexport { foo };");
        assert_eq!(d.len(), 1);
    }

    // --- Biome valid fixtures (must not fire) ---

    #[test]
    fn allows_different_symbol_biome_different_symbol() {
        // export const myFunc = () => {}; export default function () {}
        assert!(run_on("export const myFunc = () => {};\nexport default function () {}").is_empty());
    }

    #[test]
    fn allows_named_and_default_function_biome_named_and_default() {
        // export const foo = 1; export { foo }; export default function foo() {}
        // The default is an inline function declaration (a fresh binding), not a
        // reference to the named `foo` — Biome does not flag this.
        let src = "export const foo = 1;\nexport { foo };\n\nexport default function foo() {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_reexport_biome_reexport() {
        // export { foo } from "./other"; const foo = 1; export default foo;
        // The named `foo` is a re-export (out of scope); the local `foo` is only
        // default-exported, so there is no redundancy.
        let src = "export { foo } from \"./other\";\n\nconst foo = 1;\nexport default foo;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_same_value_biome_same_value() {
        // export const foo = 42; export default 42;
        assert!(run_on("export const foo = 42;\nexport default 42;").is_empty());
    }

    #[test]
    fn allows_same_value_different_binding_biome_same_value_different_binding() {
        // const foo = 1; export { foo }; const bar = foo; export default bar;
        // `bar` is a distinct binding from the named `foo`, so the default is
        // not redundant (matching is by binding, not by name).
        let src = "const foo = 1;\nexport { foo };\n\nconst bar = foo;\nexport default bar;";
        assert!(run_on(src).is_empty());
    }

    // --- Extra coverage ---

    #[test]
    fn allows_anonymous_arrow_default() {
        assert!(run_on("export const foo = 1;\nexport default () => {};").is_empty());
    }

    #[test]
    fn allows_anonymous_function_default() {
        assert!(run_on("export const foo = 1;\nexport default function () {};").is_empty());
    }

    #[test]
    fn allows_default_of_non_named_exported_identifier() {
        // `foo` is declared but never named-exported — only default-exported.
        assert!(run_on("const foo = 1;\nexport default foo;").is_empty());
    }

    #[test]
    fn allows_export_default_only() {
        assert!(run_on("export default 42;").is_empty());
    }

    #[test]
    fn fires_export_const_then_default() {
        // The canonical case from the rule docs.
        let d = run_on("export const foo = 42;\nexport default foo;");
        assert_eq!(d.len(), 1);
    }
}
