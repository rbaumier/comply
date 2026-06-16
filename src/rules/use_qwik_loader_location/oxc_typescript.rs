//! OxcCheck backend for use-qwik-loader-location.
//!
//! Walks the module once. Calls to a Qwik loader/action helper
//! (`routeLoader$`, `routeAction$`, `globalAction$`) — resolved to a named
//! import from a Qwik package — are checked, in order, for:
//!
//! 1. **Location** (route helpers only): the file must be a route boundary
//!    (`index`/`layout`/`plugin` under `src/routes`, matched via configurable
//!    globs). `globalAction$` is exempt.
//! 2. **Declaration shape**: the call must initialize a plain `const <id>`.
//! 3. **Naming**: the binding must start with `use`.
//! 4. **Export**: the binding must be exported (inline `export const` or a
//!    later `export { x }`).
//! 5. **Inline argument**: the first argument must not be a bare reference.
//!
//! The Qwik import gate prevents firing on a same-named local helper.

use std::sync::Arc;

use globset::{Glob, GlobMatcher};
use oxc_ast::ast::{
    Argument, BindingPattern, Declaration, Expression, ImportDeclarationSpecifier, ModuleExportName,
    Statement, VariableDeclarator,
};
use oxc_span::Span;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};

/// Qwik helpers whose call site must additionally live in a route boundary file.
const ROUTE_FNS: &[&str] = &["routeLoader$", "routeAction$"];

/// All Qwik loader/action helpers governed by the rule.
const LINTER_FNS: &[&str] = &["routeLoader$", "routeAction$", "globalAction$"];

/// Module specifiers that export the Qwik loader/action helpers.
const QWIK_PACKAGES: &[&str] = &["@builder.io/qwik-city", "@builder.io/qwik", "@qwik.dev/router"];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every firing path requires a call to one of these helpers.
        Some(LINTER_FNS)
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let body = &semantic.nodes().program().body;

        // Gate: local names bound to a Qwik helper via a named import. Maps the
        // local binding (possibly aliased) to the original helper name.
        let imported = collect_qwik_imports(body);
        if imported.is_empty() {
            return Vec::new();
        }

        // `export { x }` / `export { x as y }` specifiers naming a local binding.
        let exported = collect_export_specifiers(body);

        let is_route_boundary = path_matches(ctx, "route_boundary_patterns");

        let mut diagnostics = Vec::new();
        for stmt in body {
            let (declaration, is_inline_export) = match stmt {
                Statement::VariableDeclaration(decl) => (Some(&**decl), false),
                Statement::ExportNamedDeclaration(export) => match &export.declaration {
                    Some(Declaration::VariableDeclaration(decl)) => (Some(&**decl), true),
                    _ => (None, false),
                },
                _ => continue,
            };
            let Some(declaration) = declaration else {
                continue;
            };

            for declarator in &declaration.declarations {
                let Some((helper, call_span, first_arg)) =
                    qwik_helper_call(declarator, &imported)
                else {
                    continue;
                };
                let site = LoaderSite {
                    helper,
                    call_span,
                    first_arg,
                    declarator,
                    is_inline_export,
                };
                if let Some(diag) = check_declarator(ctx, &site, is_route_boundary, &exported) {
                    diagnostics.push(diag);
                }
            }
        }
        diagnostics
    }
}

/// A Qwik helper call bound to one declarator, with the module-level facts
/// needed to judge it.
struct LoaderSite<'a> {
    helper: &'static str,
    call_span: Span,
    first_arg: Option<&'a Argument<'a>>,
    declarator: &'a VariableDeclarator<'a>,
    is_inline_export: bool,
}

/// Apply the ordered location/shape/name/export/argument checks to one
/// `const <id> = <helper>(...)` declarator, returning the first violation.
fn check_declarator(
    ctx: &CheckCtx,
    site: &LoaderSite,
    is_route_boundary: bool,
    exported: &FxHashSet<String>,
) -> Option<Diagnostic> {
    let LoaderSite {
        helper,
        call_span,
        first_arg,
        declarator,
        is_inline_export,
    } = *site;
    let helper: &str = helper;
    // 1. Route helpers must live in a route boundary file.
    if ROUTE_FNS.contains(&helper) && !is_route_boundary {
        return Some(diag(
            ctx,
            call_span,
            format!(
                "The route function {helper}() has been declared outside of the route boundaries. \
                 Move it to an `index`, `layout`, or `plugin` file inside `src/routes`, or re-export it from within the route boundary."
            ),
        ));
    }

    // 2. The binding must be a plain identifier.
    let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
        return Some(diag(
            ctx,
            call_span,
            format!(
                "The loader function {helper}() is not being exported. \
                 A loader must be assigned to an exported `const` binding, or it will not run."
            ),
        ));
    };
    let name = binding.name.as_str();

    // 3. The exported name must follow the `use*` convention.
    if !name.starts_with("use") {
        return Some(diag(
            ctx,
            binding.span,
            format!(
                "The exported name of {helper}() must follow the `use*` naming convention. \
                 Rename the declaration to start with `use`."
            ),
        ));
    }

    // 4. The binding must be exported.
    if !is_inline_export && !exported.contains(name) {
        return Some(diag(
            ctx,
            binding.span,
            format!(
                "The loader function {helper}() is not being exported. \
                 A loader must be exported, or it will not run."
            ),
        ));
    }

    // 5. The first argument must be inlined, not a bare reference.
    if let Some(Argument::Identifier(reference)) = first_arg {
        return Some(diag(
            ctx,
            reference.span,
            format!(
                "It is recommended to inline the arrow function passed to {helper}() instead of passing a reference. \
                 An inline arrow function lets the optimizer keep server code out of the client build."
            ),
        ));
    }

    None
}

/// If `declarator` is `<id> = <helper>(...)` where `<helper>` resolves through
/// `imported` to a Qwik helper, return the helper's original name, the call
/// span, and the first argument.
fn qwik_helper_call<'a>(
    declarator: &'a VariableDeclarator<'a>,
    imported: &FxHashMap<String, &'static str>,
) -> Option<(&'static str, Span, Option<&'a Argument<'a>>)> {
    let Some(Expression::CallExpression(call)) = &declarator.init else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    let helper = imported.get(callee.name.as_str())?;
    Some((helper, call.span, call.arguments.first()))
}

/// Local binding names introduced by a named import of a Qwik helper, mapped to
/// the helper's original (source) name. Honors `import { routeLoader$ as foo }`.
fn collect_qwik_imports(body: &[Statement]) -> FxHashMap<String, &'static str> {
    let mut imports = FxHashMap::default();
    for stmt in body {
        let Statement::ImportDeclaration(import) = stmt else {
            continue;
        };
        if import.import_kind.is_type() {
            continue;
        }
        if !QWIK_PACKAGES.contains(&import.source.value.as_str()) {
            continue;
        }
        let Some(specifiers) = &import.specifiers else {
            continue;
        };
        for spec in specifiers {
            let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                continue;
            };
            if named.import_kind.is_type() {
                continue;
            }
            let source_name = module_export_name(&named.imported);
            if let Some(helper) = LINTER_FNS.iter().find(|h| **h == source_name) {
                imports.insert(named.local.name.as_str().to_owned(), *helper);
            }
        }
    }
    imports
}

/// Local binding names re-exported via `export { x }` / `export { x as y }`.
fn collect_export_specifiers(body: &[Statement]) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    for stmt in body {
        let Statement::ExportNamedDeclaration(export) = stmt else {
            continue;
        };
        // `export { x } from "mod"` re-exports another module's binding.
        if export.source.is_some() {
            continue;
        }
        for spec in &export.specifiers {
            if let ModuleExportName::IdentifierReference(reference) = &spec.local {
                names.insert(reference.name.as_str().to_owned());
            }
        }
    }
    names
}

fn module_export_name<'a>(name: &'a ModuleExportName<'a>) -> &'a str {
    match name {
        ModuleExportName::IdentifierName(id) => id.name.as_str(),
        ModuleExportName::IdentifierReference(reference) => reference.name.as_str(),
        ModuleExportName::StringLiteral(s) => s.value.as_str(),
    }
}

/// True when the file path matches any configured glob for `key`. An invalid
/// glob in config is skipped rather than aborting the rule.
fn path_matches(ctx: &CheckCtx, key: &str) -> bool {
    let path = ctx.path.to_string_lossy();
    ctx.config
        .string_list(super::META.id, key, ctx.lang)
        .iter()
        .filter_map(|p| Glob::new(p).ok().map(|g| g.compile_matcher()))
        .any(|matcher: GlobMatcher| matcher.is_match(path.as_ref()))
}

fn diag(ctx: &CheckCtx, span: Span, message: String) -> Diagnostic {
    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
    Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message,
        severity: Severity::Warning,
        span: Some((span.start as usize, span.size() as usize)),
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
