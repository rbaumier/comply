//! drizzle-prefer-findmany-relations OXC backend — flag `.leftJoin(` / `.innerJoin(` /
//! `.rightJoin(` / `.fullJoin(` method calls when the file also contains `relations(`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const JOIN_METHODS: &[&str] = &["leftJoin", "innerJoin", "rightJoin", "fullJoin"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["leftJoin", "innerJoin", "rightJoin", "fullJoin"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let member = match &call.callee {
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            Expression::ComputedMemberExpression(m) => {
                if let Expression::StringLiteral(s) = &m.expression {
                    s.value.as_str()
                } else {
                    return;
                }
            }
            _ => return,
        };

        if !JOIN_METHODS.contains(&member) {
            return;
        }

        // Only warn when the file actually has Drizzle relations available
        if !ctx.source_contains("relations(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Manual `.{member}(...)` chain — prefer `db.query.X.findMany({{ with: {{ ... }} }})` when relations are defined."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_left_join_with_relations_defined() {
        let src = "export const userRelations = relations(users, ({ many }) => ({ posts: many(posts) }));\n\
                   const r = db.select().from(users).leftJoin(posts, eq(users.id, posts.userId))";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_inner_join_with_relations_defined() {
        let src = "export const userRelations = relations(users, ({ many }) => ({ posts: many(posts) }));\n\
                   const r = db.select().from(users).innerJoin(posts, eq(users.id, posts.userId))";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_query_findmany_with() {
        let src = "const r = db.query.users.findMany({ with: { posts: true } })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_join_when_no_relations_defined() {
        // REVIEW regression: without `relations()` in the file, the
        // manual join is the only option — don't flag it.
        let src = "const r = db.select().from(users).leftJoin(posts, eq(users.id, posts.userId))";
        assert!(run(src).is_empty());
    }
}
