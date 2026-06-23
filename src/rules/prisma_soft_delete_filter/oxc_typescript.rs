//! prisma-soft-delete-filter oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const FIND_METHODS: &[&str] = &["findMany", "findFirst", "findUnique"];

/// True when the result of the query at `call_span` is bound to a variable that
/// is later guarded by an explicit `deletedAt` null-check in the same scope.
///
/// The intentional pattern the rule must not flag:
/// ```ignore
/// const env = await prisma.env.findFirst({ where: { id } });
/// if (!env || env.project.deletedAt !== null) return notAuthorized();
/// ```
/// Here the `deletedAt: null` filter is *deliberately* omitted from the `where`
/// clause so the code can distinguish "soft-deleted" from "not found" and return
/// a specific response — the soft-delete check is performed explicitly on the
/// result instead.
///
/// Detection is structural, not name-based: the call must be the initializer of
/// a `VariableDeclarator` (directly or under an `await`), and one of that
/// binding's resolved references must root a `StaticMemberExpression` chain whose
/// final property is `deletedAt` (`result.deletedAt`, `result.project.deletedAt`)
/// used in a guard context — a `=== null` / `!== null` comparison, or a
/// truthiness/negation check. When the result isn't bound to a variable the
/// pattern can't apply and existing behaviour is preserved.
fn result_has_post_query_deleted_at_guard(
    call_span: oxc_span::Span,
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(symbol_id) = result_binding_symbol(call_span, node, semantic) else {
        return false;
    };
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    scoping.get_resolved_references(symbol_id).any(|reference| {
        reference_roots_deleted_at_guard(reference.node_id(), semantic, nodes)
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
/// `StaticMemberExpression` chain ending in `.deletedAt`, and that `deletedAt`
/// access is used in a guard context (null comparison or truthiness/negation).
fn reference_roots_deleted_at_guard(
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
    nodes: &oxc_semantic::AstNodes,
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
        if member.property.name.as_str() == "deletedAt" {
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
        // When a schema.prisma is available, skip models that don't have a
        // `deletedAt` field — they can't have soft-deleted rows, so the
        // missing filter is not a bug. Fall through when no schema is found
        // (backward-compatible: fire on all).
        if let Expression::StaticMemberExpression(inner) = &member.object {
            let model_name = inner.property.name.as_str();
            if ctx.project.prisma_model_is_soft_delete(ctx.path, model_name) == Some(false) {
                // A schema governs this file's package and this model has no
                // `deletedAt` column — there are no soft-deleted rows, so the
                // missing filter is not a bug. (`None` = no schema → fall
                // through to fire-on-all; `Some(true)` = the model is
                // soft-delete → fall through to flag.)
                return;
            }
        }
        // Heuristic: scan the entire call source range for
        // `deletedAt` — present anywhere in the where clause is fine.
        let span_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if span_text.contains("deletedAt") {
            return;
        }
        // The `deletedAt: null` filter may be deliberately omitted so the code
        // can distinguish "deleted" from "not found" and handle the soft-deleted
        // case with an explicit post-query `result.deletedAt` guard. Suppress
        // when the bound result is guarded that way later in the same scope.
        if result_has_post_query_deleted_at_guard(call.span, node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{method}` without a `deletedAt` filter — soft-deleted rows will \
                 leak into the result. Add `where: {{ deletedAt: null, … }}`."
            ),
            severity: Severity::Warning,
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
