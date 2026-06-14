use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, cached_file_bool, SLOT_TYPE_ONLY_FILE};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, Statement, TSModuleDeclarationBody, TSType};
use std::sync::Arc;

pub struct Check;

/// True when every top-level statement is purely type-level: the file declares
/// no runtime value, imports none, and executes nothing. This is the structure
/// of a `.d.ts` ambient declaration file expressed in a `.ts`/`.tsx` source —
/// e.g. a framework's JSX intrinsic-element / ARIA attribute type definitions,
/// where `?: T | undefined` is deliberate DefinitelyTyped-style documentation
/// rather than a redundant union, so the rule must not flag it.
fn is_type_declaration_only_file(semantic: &oxc_semantic::Semantic) -> bool {
    let mut saw_type_decl = false;
    for node in semantic.nodes().iter() {
        if let AstKind::Program(prog) = node.kind() {
            for stmt in &prog.body {
                if !statement_is_type_only(stmt) {
                    return false;
                }
                if !matches!(stmt, Statement::EmptyStatement(_)) {
                    saw_type_decl = true;
                }
            }
            // An empty (or whitespace-only) file is not a type-definition file.
            return saw_type_decl;
        }
    }
    false
}

/// True when `stmt` introduces no runtime value — a type alias/interface, an
/// `import type`, an `export type`, an ambient/type-only namespace, or an empty
/// statement. A value declaration (`const`/`function`/`class`/`enum`), a value
/// import, or any executable statement makes the file an implementation file.
fn statement_is_type_only(stmt: &Statement) -> bool {
    match stmt {
        Statement::EmptyStatement(_)
        | Statement::TSTypeAliasDeclaration(_)
        | Statement::TSInterfaceDeclaration(_)
        | Statement::TSImportEqualsDeclaration(_)
        | Statement::TSExportAssignment(_)
        | Statement::TSNamespaceExportDeclaration(_) => true,
        Statement::TSModuleDeclaration(module) => module_is_type_only(module),
        Statement::ImportDeclaration(import) => import.import_kind.is_type(),
        Statement::ExportAllDeclaration(export) => export.export_kind.is_type(),
        Statement::ExportNamedDeclaration(export) => {
            if export.export_kind.is_type() {
                return true;
            }
            match &export.declaration {
                Some(decl) => declaration_is_type_only(decl),
                // `export type { foo }` is covered by `export_kind`; a re-export
                // `export { foo }` of a value binding is not type-only.
                None => false,
            }
        }
        Statement::ExportDefaultDeclaration(_) => false,
        _ => false,
    }
}

/// True when an exported `Declaration` is purely type-level.
fn declaration_is_type_only(decl: &Declaration) -> bool {
    match decl {
        Declaration::TSTypeAliasDeclaration(_)
        | Declaration::TSInterfaceDeclaration(_)
        | Declaration::TSImportEqualsDeclaration(_) => true,
        Declaration::TSModuleDeclaration(module) => module_is_type_only(module),
        _ => false,
    }
}

/// True when a `namespace`/`module` body contains only type-level statements.
/// A `declare` module is type-only by definition; a value-bearing namespace
/// (one whose body declares runtime values) is not.
fn module_is_type_only(module: &oxc_ast::ast::TSModuleDeclaration) -> bool {
    if module.declare {
        return true;
    }
    match &module.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            block.body.iter().all(statement_is_type_only)
        }
        // A nested `namespace A.B` with no block, or no body at all, declares
        // no runtime value.
        Some(TSModuleDeclarationBody::TSModuleDeclaration(inner)) => module_is_type_only(inner),
        None => true,
    }
}

/// True when the type is a *union* that includes `undefined` alongside at
/// least one other member — only then is `| undefined` redundant with `?`.
///
/// A bare `?: undefined` (the whole annotation is `undefined`, not a union)
/// is a meaningful constraint, not redundancy: the property may be absent or
/// explicitly `undefined`, but never any other value. Removing either token
/// changes the type, so the rule must not flag it.
fn union_redundantly_has_undefined(ty: &TSType) -> bool {
    if !matches!(ty, TSType::TSUnionType(_)) {
        return false;
    }
    let (mut has_undefined, mut has_other) = (false, false);
    collect_union_members(ty, &mut has_undefined, &mut has_other);
    has_undefined && has_other
}

fn collect_union_members(ty: &TSType, has_undefined: &mut bool, has_other: &mut bool) {
    match ty {
        TSType::TSUnionType(union) => {
            for t in &union.types {
                collect_union_members(t, has_undefined, has_other);
            }
        }
        TSType::TSUndefinedKeyword(_) => *has_undefined = true,
        _ => *has_other = true,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::TSPropertySignature,
            AstType::PropertyDefinition,
            AstType::FormalParameter,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (is_optional, type_ann, span_start) = match node.kind() {
            AstKind::TSPropertySignature(sig) => {
                (sig.optional, sig.type_annotation.as_ref(), sig.span.start)
            }
            AstKind::PropertyDefinition(def) => {
                (def.optional, def.type_annotation.as_ref(), def.span.start)
            }
            AstKind::FormalParameter(param) => {
                (param.optional, param.type_annotation.as_ref(), param.span.start)
            }
            _ => return,
        };

        if !is_optional {
            return;
        }

        let Some(type_ann) = type_ann else { return };
        if !union_redundantly_has_undefined(&type_ann.type_annotation) {
            return;
        }

        // Under `exactOptionalPropertyTypes`, `?: T` and `?: T | undefined`
        // diverge: only the latter accepts an explicit `undefined`. The union
        // member is then meaningful, not redundant, so suppress the diagnostic.
        if ctx.project.uses_exact_optional_property_types(ctx.path) {
            return;
        }

        // A type-declaration-only file (a `.d.ts`-equivalent: only interfaces,
        // type aliases, and type-only imports/exports) uses the explicit
        // `?: T | undefined` form as DefinitelyTyped-style documentation, e.g.
        // a framework's JSX intrinsic-element / ARIA attribute definitions.
        // There the union member is intentional, not redundant.
        if cached_file_bool(ctx.source, SLOT_TYPE_ONLY_FILE, || {
            is_type_declaration_only_file(semantic)
        }) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`?:` already implies `| undefined` — remove the redundant union member."
                .into(),
            severity: super::META.severity,
            span: None,
        });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    /// Run the rule against `source` inside a TempDir that also holds a
    /// `tsconfig.json` with the given `compiler_options` body, so the predicate
    /// `uses_exact_optional_property_types` resolves against a real tsconfig.
    fn run_with_tsconfig(source: &str, compiler_options: &str) -> Vec<Diagnostic> {
        use crate::project::ProjectCtx;
        use crate::rules::file_ctx::FileCtx;
        use crate::files::Language;

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            format!(r#"{{"compilerOptions":{compiler_options}}}"#),
        )
        .unwrap();
        let file_path = dir.path().join("t.ts");
        std::fs::write(&file_path, source).unwrap();

        let project = ProjectCtx::empty();
        let file = FileCtx::build(&file_path, source, Language::TypeScript, &project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &file_path, &project, &file)
    }

    #[test]
    fn allows_optional_with_undefined_under_exact_optional() {
        // Regression #2075: with exactOptionalPropertyTypes, `?: T | undefined`
        // is NOT redundant — the union member additionally permits an explicit
        // `undefined`, which `?` alone does not. The runtime `const` makes this
        // an implementation file, so only the exact-optional rule suppresses.
        let src = "export const SCHEMA_VERSION = 1;\n\
                   export interface JSONSchemaMeta { id?: string | undefined; }";
        assert!(run_with_tsconfig(src, r#"{"exactOptionalPropertyTypes":true}"#).is_empty());
    }

    #[test]
    fn flags_optional_with_undefined_when_exact_optional_off() {
        // Guard: without the option the union member IS redundant — true
        // positive must still fire. The runtime `const` makes this an
        // implementation file (not a type-declaration-only file).
        let src = "export const SCHEMA_VERSION = 1;\n\
                   export interface JSONSchemaMeta { id?: string | undefined; }";
        assert_eq!(
            run_with_tsconfig(src, r#"{"exactOptionalPropertyTypes":false}"#).len(),
            1
        );
    }

    #[test]
    fn flags_optional_with_undefined() {
        assert_eq!(
            run_on("const x = 1; interface I { name?: string | undefined; }").len(),
            1
        );
    }

    #[test]
    fn flags_optional_with_undefined_complex() {
        assert_eq!(
            run_on("const x = 1; interface I { value?: number | null | undefined; }").len(),
            1
        );
    }

    #[test]
    fn allows_optional_without_undefined() {
        assert!(run_on("interface I { name?: string; }").is_empty());
    }

    #[test]
    fn allows_required_with_undefined() {
        assert!(run_on("interface I { name: string | undefined; }").is_empty());
    }

    #[test]
    fn allows_optional_bare_undefined() {
        // Regression for issue #557: `?: undefined` is a meaningful constraint
        // (absent or `undefined`, never another value), not redundancy.
        assert!(run_on("interface I { type?: undefined; }").is_empty());
    }

    #[test]
    fn allows_optional_bare_undefined_in_generic_callback() {
        let src = "const mock = vi.fn<(event: { type?: undefined }) => unknown>((event) => event);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_explicit_undefined_in_type_declaration_only_file() {
        // Regression #1998: vuejs/core packages/runtime-dom/src/jsx.ts is a
        // faithful port of DefinitelyTyped's React JSX types. The file declares
        // only interfaces (no runtime content) and uses the explicit
        // `?: T | undefined` form as deliberate documentation. The rule must
        // not flag it.
        let src = "import type * as CSS from 'csstype'\n\
                   export interface AriaAttributes {\n\
                     'aria-activedescendant'?: string | undefined\n\
                     'aria-atomic'?: boolean | undefined\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_explicit_undefined_in_jsx_namespace_only_file() {
        // The JSX intrinsic-element augmentation lives in a `namespace JSX`
        // whose body is entirely interfaces — still a type-declaration-only
        // file.
        let src = "export namespace JSX {\n\
                     export interface IntrinsicElements {\n\
                       div?: { id?: string | undefined }\n\
                     }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_explicit_undefined_when_file_has_runtime_content() {
        // Negative-space guard: the same redundant `?: T | undefined` in a file
        // that also declares a runtime value is an implementation file, not a
        // type-definition file, so the true positive must still fire.
        let src = "export const VERSION = '1.0.0'\n\
                   export interface AriaAttributes {\n\
                     'aria-atomic'?: boolean | undefined\n\
                   }";
        assert_eq!(run_on(src).len(), 1);
    }
}
