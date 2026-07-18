//! prisma-select-only-needed-fields OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

fn object_has_key(obj: &oxc_ast::ast::ObjectExpression, name: &str) -> bool {
    for prop in &obj.properties {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            let key_name = match &p.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if key_name == name {
                return true;
            }
        }
    }
    false
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        if !is_prisma_file(ctx.source) {
            return;
        }

        // Callee must be a member expression with `.findMany`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "findMany" {
            return;
        }
        // Only a Prisma model delegate (`<client>.<model>.findMany(...)`) is a
        // real query; a wrapper self-call like `this.findMany(...)` selects its
        // columns in the underlying delegate call and must not be flagged.
        if !crate::oxc_helpers::is_prisma_delegate_call(member) {
            return;
        }

        // Find object expression arguments
        let obj_args: Vec<&oxc_ast::ast::ObjectExpression> = call
            .arguments
            .iter()
            .filter_map(|arg| match arg {
                Argument::ObjectExpression(obj) => Some(obj.as_ref()),
                _ => None,
            })
            .collect();

        if obj_args.is_empty() {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`findMany()` without `select` fetches every column — add `select: { ... }` for the fields you need."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        for obj in obj_args {
            if !object_has_key(obj, "select")
                && !object_has_key(obj, "include")
            {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`findMany()` is missing `select`/`include` — fetches every column."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }
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
    fn flags_delegate_findmany_without_select() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ where: { active: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_delegate_findmany_with_select() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ select: { id: true } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_delegate_findmany_with_include() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ include: { posts: true } });";
        assert!(run(src).is_empty());
    }

    // Regression for #7807: `this.findMany(...)` is a base-service wrapper
    // method, not a Prisma delegate call, so its missing `select` is not a bug.
    #[test]
    fn ignores_wrapper_self_call_this_findmany() {
        let src = "import { PrismaClient } from '@prisma/client';\nexport class Repo { async load() { return this.findMany({ where: { active: true } }); } }";
        assert!(run(src).is_empty());
    }

    // A bare-identifier receiver (`repo.findMany(...)`) is likewise not a
    // delegate call.
    #[test]
    fn ignores_wrapper_self_call_repo_findmany() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst rows = await repo.findMany({ where: { active: true } });";
        assert!(run(src).is_empty());
    }

    // A genuine delegate call through an injected client
    // (`this.prisma.<model>.findMany`) is still flagged when it lacks `select`.
    #[test]
    fn flags_this_prisma_delegate_findmany_without_select() {
        let src = "import { PrismaClient } from '@prisma/client';\nexport class Repo { async load() { return this.prisma.user.findMany({ where: { active: true } }); } }";
        assert_eq!(run(src).len(), 1);
    }

    // `prisma["user"].findMany(...)` is a delegate call (computed model
    // accessor) and still flagged when it lacks `select`.
    #[test]
    fn flags_computed_delegate_findmany_without_select() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma[\"user\"].findMany({ where: { active: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_prisma_files() {
        let src = "const rows = client.user.findMany({ where: { active: true } });";
        assert!(run(src).is_empty());
    }
}
