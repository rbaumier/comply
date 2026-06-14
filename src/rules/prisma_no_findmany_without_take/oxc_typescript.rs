//! prisma-no-findmany-without-take oxc backend — flag `*.findMany(...)` calls
//! whose options object lacks a `take:` or `first:` key.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

/// A `take:` or `first:` key bounds the result set.
fn object_is_bounded(expr: &Expression) -> bool {
    let Expression::ObjectExpression(obj) = expr else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            p.key
                .name()
                .is_some_and(|n| n == "take" || n == "first")
        } else {
            false
        }
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findMany"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !is_prisma_file(ctx.source) {
            return;
        }

        // Callee must be `*.findMany`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "findMany" {
            return;
        }

        // Bounded when any object argument carries `take:` or `first:`.
        let bounded = call
            .arguments
            .iter()
            .filter_map(|arg| arg.as_expression())
            .any(object_is_bounded);
        if bounded {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`findMany()` without `take`/`first` returns unbounded results — add a row limit."
                .into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_findmany_without_take() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ where: { active: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_findmany_no_args() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_findmany_with_take() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ take: 50 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_findmany_with_first() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ first: 50 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_prisma_files() {
        let src = "const rows = client.user.findMany();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_findmany_in_jsdoc_example() {
        // Issue #2387: `.findMany()` inside a JSDoc `@example` block is comment
        // content, not an executable call expression — must not be flagged.
        let src = r#"import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();

/**
 * Executes a function with query tags.
 *
 * @example
 * ```ts
 * const posts = await withQueryTags(
 *   { route: '/api/posts', user: 'user-123' },
 *   () => prisma.post.findMany()
 * )
 * ```
 */
export function queryTags() {}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_findmany_in_template_literal() {
        // Issue #2387: `.findMany()` inside a (tagged) template-literal string is
        // documentation text generated at runtime, not a call expression.
        let src = r#"import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();

function docComment(strings: TemplateStringsArray, ...values: string[]): string {
  return strings.join('');
}

export function example(model: string): string {
  return docComment`
    @example
    \`\`\`
    // Fetch zero or more records
    const records = await prisma.${model}.findMany()
    \`\`\`
  `;
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_real_findmany_alongside_jsdoc_example() {
        // Negative-space guard: a real, executable `.findMany()` without `take`
        // that follows a doc example is still flagged exactly once.
        let src = r#"import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();

/**
 * @example
 * ```ts
 * const users = await prisma.user.findMany()
 * ```
 */
export async function load() {
  return prisma.user.findMany({ where: { active: true } });
}"#;
        assert_eq!(run(src).len(), 1);
    }
}
