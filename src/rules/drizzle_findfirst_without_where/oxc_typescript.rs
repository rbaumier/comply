//! drizzle-findfirst-without-where oxc backend — flag `db.query.<table>.findFirst()`
//! whose options don't include `where:`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn callee_is_findfirst(callee: &Expression, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = callee else { return false };
    if member.property.name.as_str() != "findFirst" {
        return false;
    }
    // Accept any `<db>.query.<table>` shape — `db`, `database`, `tx`, `trx`,
    // `args.database`, `handle.database`, etc. are all valid Drizzle db handles.
    let obj_span = member.object.span();
    let obj_text = &source[obj_span.start as usize..obj_span.end as usize];
    obj_text.contains(".query.")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findFirst"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !callee_is_findfirst(&call.callee, ctx.source) {
            return;
        }
        // Inspect the first argument's object-literal properties. The
        // `where` key counts whether written as `where: filter`, the
        // shorthand `where`, or spread (`...filters` — we can't see
        // inside, so play safe and skip).
        let Some(first_arg) = call.arguments.first() else { return };
        let oxc_ast::ast::Argument::ObjectExpression(obj) = first_arg else { return };
        // In test files, `findFirst({})` with an empty object is intentional —
        // the test wants any row without caring which one (e.g. post-import
        // assertions).
        if ctx.file.path_segments.in_test_dir && obj.properties.is_empty() {
            return;
        }
        let mut has_where = false;
        for prop in obj.properties.iter() {
            match prop {
                ObjectPropertyKind::ObjectProperty(p) => {
                    if let PropertyKey::StaticIdentifier(id) = &p.key
                        && id.name.as_str() == "where"
                    {
                        has_where = true;
                        break;
                    }
                    if let PropertyKey::Identifier(id) = &p.key
                        && id.name.as_str() == "where"
                    {
                        has_where = true;
                        break;
                    }
                }
                // Spread element — we can't see through `...x`, assume
                // it might carry `where` and skip the diagnostic.
                ObjectPropertyKind::SpreadProperty(_) => {
                    has_where = true;
                    break;
                }
            }
        }
        if has_where {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.findFirst()` without `where:` returns an arbitrary row — pass a filter to scope the query.".into(),
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_in_test_file(src: &str) -> Vec<crate::diagnostic::Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn flags_findfirst_no_where_inline() {
        assert_eq!(
            run("const u = await db.query.users.findFirst({ columns: { id: true } });").len(),
            1
        );
    }

    #[test]
    fn allows_findfirst_with_where_value() {
        assert!(
            run("const u = await db.query.users.findFirst({ where: eq(users.id, id) });").is_empty()
        );
    }

    // Regression for rbaumier/comply#81 — shorthand `where` must be recognised
    // by the OXC backend, not just the tree-sitter backend.
    #[test]
    fn allows_findfirst_with_shorthand_where() {
        assert!(
            run("const u = await db.query.users.findFirst({ where, with: { posts: true } });")
                .is_empty()
        );
    }

    #[test]
    fn allows_findfirst_with_spread() {
        assert!(run("const u = await db.query.users.findFirst({ ...opts });").is_empty());
    }

    #[test]
    fn ignores_non_drizzle_findfirst() {
        assert!(run("arr.findFirst({ where: eq() });").is_empty());
    }

    // Regression for rbaumier/comply#357 — `database.query.*` handle (not `db.query.*`)
    // with shorthand `where` must not be flagged.
    #[test]
    fn allows_database_handle_with_shorthand_where() {
        assert!(
            run("database.query.organization.findFirst({ where, with: { teams: true } });")
                .is_empty()
        );
    }

    // Regression for rbaumier/comply#357 — nested handle `args.database.query.*`
    // with shorthand `where` must not be flagged.
    #[test]
    fn allows_nested_database_handle_with_shorthand_where() {
        assert!(
            run("args.database.query.team.findFirst({ where, columns: { id: true } });")
                .is_empty()
        );
    }

    // Regression for rbaumier/comply#357 — `database.query.*` without `where` must be flagged.
    #[test]
    fn flags_database_handle_without_where() {
        assert_eq!(
            run("database.query.organization.findFirst({ columns: { id: true } });").len(),
            1
        );
    }

    // Regression for rbaumier/comply#530 — `findFirst({})` with empty object in test files is
    // intentional (fetch any row for post-import assertions).
    #[test]
    fn no_fp_findfirst_empty_object_in_test_file() {
        let src = "const anyRow = await db.query.team.findFirst({});";
        assert!(run_in_test_file(src).is_empty());
    }

    // `findFirst({})` in production code is still flagged.
    #[test]
    fn flags_findfirst_empty_object_in_production() {
        let src = "const anyRow = await db.query.team.findFirst({});";
        assert_eq!(run(src).len(), 1);
    }

    // `findFirst({ columns: {...} })` without `where` in test files is still flagged.
    #[test]
    fn flags_findfirst_with_options_no_where_in_test_file() {
        let src = "const u = await db.query.users.findFirst({ columns: { id: true } });";
        assert_eq!(run_in_test_file(src).len(), 1);
    }
}
