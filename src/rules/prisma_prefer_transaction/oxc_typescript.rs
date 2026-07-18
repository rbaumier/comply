//! prisma-prefer-transaction oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;
use std::sync::Arc;

const WRITE_METHODS: &[&str] = &[
    "create", "createMany", "update", "updateMany", "delete", "deleteMany", "upsert",
];

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

/// True when `call` is a Prisma model write: `prisma.<model>.create(...)` /
/// `prisma["<model>"].update(...)` etc. The method must be a write method and
/// the call must hang off a model-delegate receiver — see
/// [`crate::oxc_helpers::is_prisma_delegate_call`] for the receiver-shape check.
fn is_prisma_model_write(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !WRITE_METHODS.contains(&member.property.name.as_str()) {
        return false;
    }
    crate::oxc_helpers::is_prisma_delegate_call(member)
}

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@prisma/client", "PrismaClient", "prisma."])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_prisma_file(ctx.source) {
            return Vec::new();
        }

        // Count Prisma model writes grouped by their nearest enclosing function.
        let mut writes_by_fn: FxHashMap<oxc_semantic::NodeId, usize> = FxHashMap::default();
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            if !is_prisma_model_write(call) {
                continue;
            }
            for ancestor in semantic.nodes().ancestors(node.id()) {
                if matches!(
                    ancestor.kind(),
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                ) {
                    *writes_by_fn.entry(ancestor.id()).or_insert(0) += 1;
                    break;
                }
            }
        }

        let mut diagnostics = Vec::new();
        for (fn_id, writes) in writes_by_fn {
            if writes < 2 {
                continue;
            }
            let span = semantic.nodes().kind(fn_id).span();

            // Already wrapped in `$transaction` — nothing to suggest.
            let body_text = &ctx.source[span.start as usize..span.end as usize];
            if body_text.contains("$transaction") {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "{writes} Prisma write calls in this function — wrap them in `prisma.$transaction([...])` for atomicity."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        // `writes_by_fn` iterates in unspecified order — sort for stable output.
        diagnostics.sort_by_key(|d| (d.line, d.column));
        diagnostics
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

    // Regression for #4684: `loader.create(prismaPlaceholder)` is a factory/DI
    // call — the Prisma client is the argument, not the model accessor. The
    // receiver is a bare identifier, so none of these count as writes.
    #[test]
    fn ignores_factory_create_with_prisma_argument() {
        let src = r#"
            import { PrismaClient } from "@prisma/client";
            const prismaPlaceholder = {} as unknown as PrismaClient;
            function test() {
                const a = loader.create(prismaPlaceholder);
                const b = loader.create(prismaPlaceholder);
                const c = loader.create(prismaPlaceholder, { forceFallback: true });
                const d = loader.create(prismaPlaceholder);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_multiple_prisma_model_writes() {
        let src = r#"
            import { PrismaClient } from "@prisma/client";
            const prisma = new PrismaClient();
            async function save() {
                await prisma.user.create({ data: {} });
                await prisma.post.update({ where: {}, data: {} });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_computed_member_model_writes() {
        let src = r#"
            import { PrismaClient } from "@prisma/client";
            const prisma = new PrismaClient();
            async function save() {
                await prisma["user"].create({ data: {} });
                await prisma["post"].delete({ where: {} });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_single_prisma_write() {
        let src = r#"
            import { PrismaClient } from "@prisma/client";
            const prisma = new PrismaClient();
            async function save() {
                await prisma.user.create({ data: {} });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_writes_already_in_transaction() {
        let src = r#"
            import { PrismaClient } from "@prisma/client";
            const prisma = new PrismaClient();
            async function save() {
                await prisma.$transaction([
                    prisma.user.create({ data: {} }),
                    prisma.post.update({ where: {}, data: {} }),
                ]);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn counts_writes_per_function_separately() {
        // Two distinct single-write functions — neither reaches the threshold.
        let src = r#"
            import { PrismaClient } from "@prisma/client";
            const prisma = new PrismaClient();
            async function createUser() {
                await prisma.user.create({ data: {} });
            }
            async function updatePost() {
                await prisma.post.update({ where: {}, data: {} });
            }
        "#;
        assert!(run(src).is_empty());
    }
}
