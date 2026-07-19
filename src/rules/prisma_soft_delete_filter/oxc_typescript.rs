//! prisma-soft-delete-filter oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::PrismaSoftDelete;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Expression, IdentifierReference, PropertyKey, TSType,
    TSTypeName,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const FIND_METHODS: &[&str] = &["findMany", "findFirst", "findUnique"];

/// True when the result of the query at `call_span` is bound to a variable that
/// is later guarded by an explicit null-check on the soft-delete `field` in the
/// same scope.
///
/// The intentional pattern the rule must not flag:
/// ```ignore
/// const env = await prisma.env.findFirst({ where: { id } });
/// if (!env || env.project.deletedAt !== null) return notAuthorized();
/// ```
/// Here the `field: null` filter is *deliberately* omitted from the `where`
/// clause so the code can distinguish "soft-deleted" from "not found" and return
/// a specific response — the soft-delete check is performed explicitly on the
/// result instead.
///
/// Detection is structural, not name-based: the call must be the initializer of
/// a `VariableDeclarator` (directly or under an `await`), and one of that
/// binding's resolved references must root a `StaticMemberExpression` chain whose
/// final property is `field` (`result.deletedAt`, `result.project.deletedAt`)
/// used in a guard context — a `=== null` / `!== null` comparison, or a
/// truthiness/negation check. When the result isn't bound to a variable the
/// pattern can't apply and existing behaviour is preserved.
fn result_has_post_query_soft_delete_guard(
    call_span: oxc_span::Span,
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    field: &str,
) -> bool {
    let Some(symbol_id) = result_binding_symbol(call_span, node, semantic) else {
        return false;
    };
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    scoping.get_resolved_references(symbol_id).any(|reference| {
        reference_roots_soft_delete_guard(reference.node_id(), semantic, nodes, field)
    })
}

/// Resolve the `SymbolId` of the variable the query result is bound to, when the
/// query `CallExpression` (at `call_span`) is the initializer of a
/// `VariableDeclarator` whose `id` is a plain identifier. The initializer may be
/// the call directly or an `await` of it (`const x = await prisma...findFirst()`).
fn result_binding_symbol(
    call_span: oxc_span::Span,
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::SymbolId> {
    let nodes = semantic.nodes();
    for kind in nodes.ancestor_kinds(node.id()) {
        match kind {
            // Skip the `await` wrapping the call, if any.
            AstKind::AwaitExpression(_) => continue,
            AstKind::VariableDeclarator(decl) => {
                let init_span = decl.init.as_ref()?.span();
                // The declarator's initializer must be exactly this query (or the
                // `await` of it, which shares the call's outer span via the
                // AwaitExpression we skipped) — guard against matching an outer
                // declarator that merely contains the call somewhere deeper.
                if !span_contains(init_span, call_span) {
                    return None;
                }
                let BindingPattern::BindingIdentifier(id) = &decl.id else {
                    return None;
                };
                return id.symbol_id.get();
            }
            // Any other enclosing expression/statement means the call result is
            // not directly bound to a variable — the guard pattern can't apply.
            _ => return None,
        }
    }
    None
}

fn span_contains(outer: oxc_span::Span, inner: oxc_span::Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// True when the reference at `ref_node_id` is the root object of a
/// `StaticMemberExpression` chain ending in `.<field>`, and that `field` access
/// is used in a guard context (null comparison or truthiness/negation).
fn reference_roots_soft_delete_guard(
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
    nodes: &oxc_semantic::AstNodes,
    field: &str,
) -> bool {
    let mut child_span = nodes.get_node(ref_node_id).kind().span();
    let mut current_id = ref_node_id;
    // Walk up the member-access chain while the current node is the `.object` of
    // a `StaticMemberExpression`. Each step extends `result -> result.project ->
    // result.project.deletedAt`.
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let AstKind::StaticMemberExpression(member) = nodes.kind(parent_id) else {
            return false;
        };
        // The reference must be the object side, not the property identifier.
        if member.object.span() != child_span {
            return false;
        }
        if member.property.name.as_str() == field {
            return member_access_in_guard_context(parent_id, member.span(), semantic);
        }
        child_span = member.span();
        current_id = parent_id;
    }
}

/// True when the `deletedAt` member access (node `member_id`, spanning
/// `member_span`) is consumed by an explicit guard: a `=== null` / `!== null`
/// (or loose `== null` / `!= null`) comparison, a `!` negation, or a direct
/// truthiness condition in an `if` / `while` / conditional / logical operand.
fn member_access_in_guard_context(
    member_id: oxc_semantic::NodeId,
    member_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::{BinaryOperator, UnaryOperator};

    let nodes = semantic.nodes();
    match nodes.kind(nodes.parent_id(member_id)) {
        AstKind::BinaryExpression(bin) => {
            matches!(
                bin.operator,
                BinaryOperator::StrictEquality
                    | BinaryOperator::StrictInequality
                    | BinaryOperator::Equality
                    | BinaryOperator::Inequality
            ) && (is_null_or_undefined(&bin.left) || is_null_or_undefined(&bin.right))
        }
        AstKind::UnaryExpression(unary) => unary.operator == UnaryOperator::LogicalNot,
        // Used directly as a condition / logical operand: `if (x.deletedAt)`,
        // `x.deletedAt && ...`, `cond ? ... : ...`.
        AstKind::IfStatement(stmt) => stmt.test.span() == member_span,
        AstKind::WhileStatement(stmt) => stmt.test.span() == member_span,
        AstKind::ConditionalExpression(cond) => cond.test.span() == member_span,
        AstKind::LogicalExpression(_) => true,
        _ => false,
    }
}

fn is_null_or_undefined(expr: &Expression) -> bool {
    matches!(expr, Expression::NullLiteral(_))
        || matches!(expr, Expression::Identifier(id) if id.name == "undefined")
}

/// Module specifier the Prisma client identifier `ident` is imported from
/// (`import { prisma } from '@scope/prisma'` → `"@scope/prisma"`). `None` when
/// the binding is unresolved or not an import — e.g. a local
/// `const prisma = new PrismaClient()` — so the schema discovery falls back to
/// the file's own package boundary.
fn client_import_specifier<'a>(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let symbol_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(symbol_id);
    // The binding declaration is an import specifier under an `ImportDeclaration`
    // (or, defensively, that declaration node itself); read its source string.
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .find_map(|kind| match kind {
            AstKind::ImportDeclaration(decl) => Some(decl.source.value.as_str()),
            _ => None,
        })
}

/// Module specifier of the schema-owning package for the Prisma client used as
/// the query receiver `<receiver>.<model>`. Covers a named client import
/// (`prisma.model.findMany()` → the `prisma` import's source) and a
/// constructor-injected NestJS service (`this.prismaService.model.findMany()`),
/// whose declared property type is resolved to its import via the same symbol +
/// import-graph provenance. `None` when the receiver is neither shape or its
/// provenance is unresolved — schema discovery then falls back to the file's own
/// package boundary.
fn client_import_specifier_of<'a>(
    receiver: &Expression<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    match receiver {
        Expression::Identifier(client) => client_import_specifier(client, semantic),
        // `this.<prop>` — an injected class property; resolve `<prop>`'s declared
        // type to the import it comes from.
        Expression::StaticMemberExpression(prop)
            if matches!(prop.object, Expression::ThisExpression(_)) =>
        {
            let type_ident =
                injected_property_type_ident(prop.property.name.as_str(), node, semantic)?;
            client_import_specifier(type_ident, semantic)
        }
        _ => None,
    }
}

/// Type-name identifier of the declared type of the injected class property
/// `prop_name`, looked up in the nearest enclosing class. Covers a constructor
/// parameter property
/// (`constructor(private readonly prismaService: PrismaService)`) and a class
/// field (`private prismaService: PrismaService`). The returned identifier is a
/// type reference resolvable to its import, so the same provenance path a named
/// client uses applies. `None` when no enclosing class declares `prop_name` with
/// a simple type annotation.
fn injected_property_type_ident<'a>(
    prop_name: &str,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a IdentifierReference<'a>> {
    let nodes = semantic.nodes();
    // `this` binds to the nearest enclosing class, so only that class can declare
    // the property — stop at the first class walking up.
    for kind in nodes.ancestor_kinds(node.id()) {
        if let AstKind::Class(class) = kind {
            return class_property_type_ident(class, prop_name);
        }
    }
    None
}

/// Type-name identifier of the declared type of the class member named
/// `prop_name` — a constructor parameter property or a class field.
fn class_property_type_ident<'a>(
    class: &'a Class<'a>,
    prop_name: &str,
) -> Option<&'a IdentifierReference<'a>> {
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(prop)
                if property_key_is(&prop.key, prop_name) =>
            {
                return type_reference_ident(&prop.type_annotation.as_ref()?.type_annotation);
            }
            ClassElement::MethodDefinition(method)
                if property_key_is(&method.key, "constructor") =>
            {
                for param in &method.value.params.items {
                    if crate::oxc_helpers::is_parameter_property(param)
                        && let BindingPattern::BindingIdentifier(id) = &param.pattern
                        && id.name.as_str() == prop_name
                        && let Some(ann) = &param.type_annotation
                    {
                        return type_reference_ident(&ann.type_annotation);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `key` is a static-identifier property key named `name`.
fn property_key_is(key: &PropertyKey, name: &str) -> bool {
    matches!(key, PropertyKey::StaticIdentifier(id) if id.name.as_str() == name)
}

/// The identifier of a simple type reference (`Foo`), or `None` for any other
/// type (union, qualified `ns.Foo`, primitive, …). The identifier carries a
/// resolvable reference id, so [`client_import_specifier`] can trace it to its
/// import.
fn type_reference_ident<'a>(ty: &'a TSType<'a>) -> Option<&'a IdentifierReference<'a>> {
    let TSType::TSTypeReference(type_ref) = ty else {
        return None;
    };
    match &type_ref.type_name {
        TSTypeName::IdentifierReference(ident) => Some(&**ident),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".findMany(", ".findFirst(", ".findUnique("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !FIND_METHODS.contains(&method) {
            return;
        }
        // Only fire when the file mentions `prisma` somewhere — keeps
        // the rule from misfiring on Drizzle / unrelated APIs that may
        // happen to expose the same method name.
        if !ctx.source_contains("prisma") {
            return;
        }
        // A Prisma delegate query is `<client>.<model>.findX(...)`. A wrapper
        // self-call (`this.findMany()`, `repo.findFirst()`) is not a delegate:
        // its receiver is `this`/a bare identifier, not a `.<model>` accessor.
        // Bail before the schema-less `deletedAt` default can fire on it — the
        // soft-delete filter belongs to the underlying delegate call inside the
        // wrapper, not to the wrapper site.
        if !crate::oxc_helpers::is_prisma_delegate_call(member) {
            return;
        }
        // The soft-delete column to enforce. Defaults to `deletedAt` for the
        // schema-less fallback; when a schema governs this query it becomes the
        // exact field the model declares, so a project whose column is
        // `deletedTime` is driven by the schema rather than this default.
        let mut soft_delete_field = String::from("deletedAt");

        // When a schema.prisma is available, consult it for this model. The
        // authoritative schema is the one backing the client this call uses:
        // a `prisma` import from a workspace package
        // (`import { prisma } from '@scope/prisma'`) or a constructor-injected
        // NestJS service (`this.prismaService.model.…`, whose declared type
        // resolves to its import) both point at that package's schema (in the
        // dominant monorepo layout the schema lives in a dedicated sibling
        // package, outside this file's own package boundary).
        if let Expression::StaticMemberExpression(inner) = &member.object {
            let model_name = inner.property.name.as_str();
            let client_specifier = client_import_specifier_of(&inner.object, node, semantic);
            match ctx
                .project
                .prisma_model_soft_delete(ctx.path, model_name, client_specifier)
            {
                // A schema governs this query and this model has no soft-delete
                // column — there are no soft-deleted rows, so the missing filter
                // is not a bug.
                Some(PrismaSoftDelete::NotSoftDelete) => return,
                // The schema declares the soft-delete column: enforce that exact
                // field name in the where clause, guard check, and message.
                Some(PrismaSoftDelete::SoftDeleteField(field)) => soft_delete_field = field,
                // No schema resolved → fall through with the default field.
                None => {}
            }
        }
        // Heuristic: scan the entire call source range for the soft-delete field —
        // present anywhere in the where clause is fine.
        let span_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if span_text.contains(soft_delete_field.as_str()) {
            return;
        }
        // The `<field>: null` filter may be deliberately omitted so the code can
        // distinguish "deleted" from "not found" and handle the soft-deleted case
        // with an explicit post-query `result.<field>` guard. Suppress when the
        // bound result is guarded that way later in the same scope.
        if result_has_post_query_soft_delete_guard(call.span, node, semantic, &soft_delete_field) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{method}` without a `{soft_delete_field}` filter — soft-deleted rows will \
                 leak into the result. Add `where: {{ {soft_delete_field}: null, … }}`."
            ),
            severity: Severity::Error,
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
    use crate::project::ProjectCtx;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_gated(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path)
    }

    fn run_with_project(src: &str, path: &std::path::Path, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, path, project, crate::rules::file_ctx::default_static_file_ctx())
    }

    /// Build a single-package project under a tempdir whose `prisma/schema.prisma`
    /// declares an `Envelope` model with `deletedAt` (and an `Account` without),
    /// returning the loaded `ProjectCtx` and a source file path inside the package.
    fn project_with_envelope_schema() -> (tempfile::TempDir, ProjectCtx, std::path::PathBuf) {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"app"}"#).unwrap();
        std::fs::create_dir_all(dir.path().join("prisma")).unwrap();
        std::fs::write(
            dir.path().join("prisma/schema.prisma"),
            "model Account {\n  id String @id\n}\n\nmodel Envelope {\n  id String @id\n  deletedAt DateTime?\n}\n",
        )
        .unwrap();
        let file_path = dir.path().join("src/repo.ts");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "export const x = 1;").unwrap();
        let source = SourceFile { path: file_path.clone(), language: Language::TypeScript };
        let ctx = ProjectCtx::load(&[&source], &Config::default());
        (dir, ctx, file_path)
    }

    /// Build a monorepo under a tempdir: a dedicated `@scope/prisma` package owns
    /// `schema.prisma` (`Recipient` without `deletedAt`, `Envelope` with it) and a
    /// consumer `@scope/lib` package has no schema of its own. Returns the loaded
    /// `ProjectCtx` and a source file path inside the consumer package.
    fn monorepo_with_sibling_prisma_schema() -> (tempfile::TempDir, ProjectCtx, std::path::PathBuf) {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        let prisma_pkg = dir.path().join("packages/prisma");
        std::fs::create_dir_all(&prisma_pkg).unwrap();
        std::fs::write(prisma_pkg.join("package.json"), r#"{"name":"@scope/prisma"}"#).unwrap();
        std::fs::write(
            prisma_pkg.join("schema.prisma"),
            "model Recipient {\n  id String @id\n  documentDeletedAt DateTime?\n}\n\nmodel Envelope {\n  id String @id\n  deletedAt DateTime?\n}\n",
        )
        .unwrap();
        let lib_pkg = dir.path().join("packages/lib");
        std::fs::create_dir_all(lib_pkg.join("src")).unwrap();
        std::fs::write(lib_pkg.join("package.json"), r#"{"name":"@scope/lib"}"#).unwrap();
        let file_path = lib_pkg.join("src/repo.ts");
        std::fs::write(&file_path, "export const x = 1;").unwrap();
        let source = SourceFile { path: file_path.clone(), language: Language::TypeScript };
        let ctx = ProjectCtx::load(&[&source], &Config::default());
        (dir, ctx, file_path)
    }

    /// Build a monorepo where a dedicated `@scope/prisma` package owns
    /// `schema.prisma` (`View` with a `deletedTime DateTime?` soft-delete column,
    /// `Account` with no soft-delete column, `Envelope` with `deletedAt`) and a
    /// consumer `@scope/app` package — with no schema of its own — injects the
    /// client as a NestJS service. Returns the loaded `ProjectCtx` and a source
    /// file path inside the consumer package.
    fn monorepo_with_injected_prisma_service() -> (tempfile::TempDir, ProjectCtx, std::path::PathBuf)
    {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        let prisma_pkg = dir.path().join("packages/prisma");
        std::fs::create_dir_all(&prisma_pkg).unwrap();
        std::fs::write(prisma_pkg.join("package.json"), r#"{"name":"@scope/prisma"}"#).unwrap();
        std::fs::write(
            prisma_pkg.join("schema.prisma"),
            "model View {\n  id String @id\n  deletedTime DateTime?\n}\n\nmodel Account {\n  id String @id\n}\n\nmodel Envelope {\n  id String @id\n  deletedAt DateTime?\n}\n",
        )
        .unwrap();
        let app_pkg = dir.path().join("packages/app");
        std::fs::create_dir_all(app_pkg.join("src")).unwrap();
        std::fs::write(app_pkg.join("package.json"), r#"{"name":"@scope/app"}"#).unwrap();
        let file_path = app_pkg.join("src/view.service.ts");
        std::fs::write(&file_path, "export const x = 1;").unwrap();
        let source = SourceFile { path: file_path.clone(), language: Language::TypeScript };
        let ctx = ProjectCtx::load(&[&source], &Config::default());
        (dir, ctx, file_path)
    }

    // Regression for #7434: the client is imported from a dedicated sibling
    // package `@scope/prisma` whose schema's `Recipient` has no `deletedAt`, so
    // the missing filter is not a bug and the query must not be flagged.
    #[test]
    fn ignores_recipient_when_client_imported_from_sibling_package() {
        let (_dir, project, file_path) = monorepo_with_sibling_prisma_schema();
        let src = r#"
            import { prisma } from '@scope/prisma';
            const r = await prisma.recipient.findMany({ where: {} });
        "#;
        assert!(run_with_project(src, &file_path, &project).is_empty());
    }

    // The same fixture: `Envelope` has `deletedAt` in `@scope/prisma`, so a
    // query that omits the filter is still flagged (no false negative).
    // Regression for #7671: the client is a constructor-injected NestJS service
    // (`this.prismaService.<model>.…`) whose declared type resolves to its import
    // from the schema-owning package. The schema must be consulted the same way a
    // named import is — otherwise every query in a DI-based backend fires. `View`
    // is a soft-delete model whose column is `deletedTime`, and this query omits
    // the filter, so it is still flagged (with the schema-derived field name).
    #[test]
    fn flags_injected_service_soft_delete_model_missing_filter() {
        let (_dir, project, file_path) = monorepo_with_injected_prisma_service();
        let src = r#"
            import { PrismaService } from '@scope/prisma';
            class ViewService {
                constructor(private readonly prismaService: PrismaService) {}
                getViews(ids: string[]) {
                    return this.prismaService.view.findMany({ where: { id: { in: ids } } });
                }
            }
        "#;
        let diags = run_with_project(src, &file_path, &project);
        assert_eq!(diags.len(), 1);
        // The message names the schema-derived field, not the hard-coded default.
        assert!(diags[0].message.contains("deletedTime"));
    }

    // The same injected service, but the query filters on the schema's
    // `deletedTime` column — the soft-delete filter is present, so it is silent.
    #[test]
    fn ignores_injected_service_soft_delete_model_with_filter() {
        let (_dir, project, file_path) = monorepo_with_injected_prisma_service();
        let src = r#"
            import { PrismaService } from '@scope/prisma';
            class ViewService {
                constructor(private readonly prismaService: PrismaService) {}
                getViews(ids: string[]) {
                    return this.prismaService.view.findMany({
                        where: { deletedTime: null, id: { in: ids } },
                    });
                }
            }
        "#;
        assert!(run_with_project(src, &file_path, &project).is_empty());
    }

    // `Account` has no soft-delete column in the resolved schema, so a query via
    // the injected service that omits any filter is not a bug and must be silent.
    #[test]
    fn ignores_injected_service_non_soft_delete_model() {
        let (_dir, project, file_path) = monorepo_with_injected_prisma_service();
        let src = r#"
            import { PrismaService } from '@scope/prisma';
            class AccountService {
                constructor(private readonly prismaService: PrismaService) {}
                getAccount(id: string) {
                    return this.prismaService.account.findFirst({ where: { id } });
                }
            }
        "#;
        assert!(run_with_project(src, &file_path, &project).is_empty());
    }

    // True-positive control via the injected service: `Envelope` is a genuine
    // `deletedAt` soft-delete model, and this query omits the filter → flagged.
    #[test]
    fn flags_envelope_via_injected_service_missing_filter() {
        let (_dir, project, file_path) = monorepo_with_injected_prisma_service();
        let src = r#"
            import { PrismaService } from '@scope/prisma';
            class EnvelopeService {
                constructor(private readonly prismaService: PrismaService) {}
                get(id: string) {
                    return this.prismaService.envelope.findFirst({ where: { id } });
                }
            }
        "#;
        let diags = run_with_project(src, &file_path, &project);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("deletedAt"));
    }

    // Resolution is structural, not name-based: an injected property named `orm`
    // typed `Client` (neither `prismaService` nor `PrismaService`) still resolves
    // to the schema package. That the message names `deletedTime` — not the
    // fall-through default `deletedAt` — proves the schema was resolved via the
    // property's declared type, not a hard-coded receiver name.
    #[test]
    fn resolves_injected_service_regardless_of_property_and_type_name() {
        let (_dir, project, file_path) = monorepo_with_injected_prisma_service();
        let src = r#"
            import { Client } from '@scope/prisma';
            class S {
                constructor(private readonly orm: Client) {}
                f(id: string) {
                    return this.orm.view.findMany({ where: { id } });
                }
            }
        "#;
        let diags = run_with_project(src, &file_path, &project);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("deletedTime"));
    }

    // Property injection (a class field rather than a constructor parameter
    // property) is the other NestJS DI shape; its declared type resolves to the
    // same schema package, so a `View` query omitting the `deletedTime` filter is
    // still flagged with the schema-derived field name.
    #[test]
    fn flags_injected_service_declared_as_class_field() {
        let (_dir, project, file_path) = monorepo_with_injected_prisma_service();
        let src = r#"
            import { PrismaService } from '@scope/prisma';
            class ViewService {
                private readonly prismaService: PrismaService;
                getViews(ids: string[]) {
                    return this.prismaService.view.findMany({ where: { id: { in: ids } } });
                }
            }
        "#;
        let diags = run_with_project(src, &file_path, &project);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("deletedTime"));
    }

    // The same schema resolved through a named `prisma` import: `View`'s
    // `deletedTime` column drives the where-check, so a `deletedTime: null`
    // filter silences the query even though the field is not `deletedAt`.
    #[test]
    fn named_import_where_check_uses_schema_derived_field() {
        let (_dir, project, file_path) = monorepo_with_injected_prisma_service();
        let src = r#"
            import { prisma } from '@scope/prisma';
            const v = await prisma.view.findMany({ where: { deletedTime: null } });
        "#;
        assert!(run_with_project(src, &file_path, &project).is_empty());
    }

    #[test]
    fn flags_envelope_when_client_imported_from_sibling_package() {
        let (_dir, project, file_path) = monorepo_with_sibling_prisma_schema();
        let src = r#"
            import { prisma } from '@scope/prisma';
            const e = await prisma.envelope.findFirst({ where: {} });
        "#;
        assert_eq!(run_with_project(src, &file_path, &project).len(), 1);
    }

    #[test]
    fn flags_find_many_without_deleted_at() {
        let src = r#"const r = await prisma.user.findMany({ where: { active: true } });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_many_with_deleted_at() {
        let src = r#"const r = await prisma.user.findMany({ where: { deletedAt: null, active: true } });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_prisma_callers() {
        let src = r#"const r = obj.findMany({ where: { active: true } });"#;
        assert!(run(src).is_empty());
    }

    // Regression for #7807: `this.findMany(...)` is an inherited base-service
    // wrapper method, not a `<client>.<model>.findX` delegate call — its
    // receiver is `this`, not a model accessor — so it must never reach the
    // schema-less `deletedAt` default and fire.
    #[test]
    fn ignores_wrapper_self_call_this_findmany() {
        let src = r#"
            import { PrismaClient } from '@prisma/client';
            export class Repo {
                async load() {
                    return this.findMany({ where: { active: true } });
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    // A bare-identifier receiver (`repo.findFirst(...)`) is likewise not a
    // delegate call.
    #[test]
    fn ignores_wrapper_self_call_repo_findfirst() {
        let src = r#"
            import { PrismaClient } from '@prisma/client';
            const record = await repo.findFirst({ where: { id: "1" } });
        "#;
        assert!(run(src).is_empty());
    }

    // `svc.findUnique(...)` — bare-identifier receiver, not a delegate call.
    #[test]
    fn ignores_wrapper_self_call_svc_findunique() {
        let src = r#"
            import { PrismaClient } from '@prisma/client';
            const record = await svc.findUnique({ where: { id: "1" } });
        "#;
        assert!(run(src).is_empty());
    }

    // A genuine delegate call through an injected client
    // (`this.prisma.<model>.findMany`) still fires on the schema-less default.
    #[test]
    fn flags_this_prisma_delegate_findmany() {
        let src = r#"
            import { PrismaClient } from '@prisma/client';
            export class Repo {
                async load() {
                    return this.prisma.user.findMany({ where: { active: true } });
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression tests for issue #836: FP on models without deletedAt field.

    #[test]
    fn ignores_find_first_on_model_without_deleted_at_in_schema() {
        // schema contains "envelope" with deletedAt, but not "user"
        let (_dir, project, file_path) = project_with_envelope_schema();
        let src =
            r#"const u = await prisma.user.findFirst({ where: { email: "x" } });"#;
        assert!(run_with_project(src, &file_path, &project).is_empty());
    }

    #[test]
    fn flags_find_first_on_model_with_deleted_at_in_schema() {
        let (_dir, project, file_path) = project_with_envelope_schema();
        let src =
            r#"const e = await prisma.envelope.findFirst({ where: { id: "1" } });"#;
        assert_eq!(run_with_project(src, &file_path, &project).len(), 1);
    }

    // Regression for issue #1358: schemas defined via TypeScript template strings
    // (no static `.prisma` file) leave the rule with no model list, so the
    // schema-less fallback fired on every query in the test suite. Soft-delete
    // enforcement is a production data-integrity concern, so the rule is gated
    // out of test directories.

    #[test]
    fn ignores_find_many_in_test_dir_without_schema() {
        let src = r#"const r = await prisma.user.findMany({ where: { active: true } });"#;
        assert!(
            run_gated(src, "packages/client/tests/functional/tests_m-to-n.ts").is_empty()
        );
    }

    #[test]
    fn flags_find_many_in_production_dir_without_schema() {
        let src = r#"const r = await prisma.user.findMany({ where: { active: true } });"#;
        assert_eq!(run_gated(src, "src/repositories/user.ts").len(), 1);
    }

    // Regression for issue #4685: a query that deliberately omits the
    // `deletedAt` filter and instead performs an explicit post-query
    // `deletedAt` guard on its result is a valid pattern (it distinguishes
    // "deleted" from "not found"), so it must not be flagged.

    #[test]
    fn ignores_find_first_with_nested_relation_deleted_at_guard() {
        let src = r#"
            async function f(envId: string) {
                const env = await prisma.runtimeEnvironment.findFirst({
                    where: { id: envId },
                    include: { project: true },
                });
                if (!env || env.project.deletedAt !== null) {
                    return { ok: false, status: 401 };
                }
                return env;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_find_first_with_direct_deleted_at_guard() {
        let src = r#"
            async function f(id: string) {
                const record = await prisma.user.findFirst({ where: { id } });
                if (record === null || record.deletedAt === null) {
                    return null;
                }
                return record;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_find_first_with_truthiness_deleted_at_guard() {
        let src = r#"
            async function f(id: string) {
                const record = await prisma.user.findFirst({ where: { id } });
                if (record && record.deletedAt) {
                    throw new Error("deleted");
                }
                return record;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_find_first_without_guard() {
        let src = r#"
            async function f(id: string) {
                const record = await prisma.user.findFirst({ where: { id } });
                return record;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_find_first_when_other_field_guarded_not_deleted_at() {
        let src = r#"
            async function f(id: string) {
                const record = await prisma.user.findFirst({ where: { id } });
                if (record && record.archivedAt !== null) {
                    return null;
                }
                return record;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_awaited_find_first_unbound() {
        let src = r#"
            async function f(id: string) {
                return (await prisma.user.findFirst({ where: { id } })).deletedAt;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
