//! prisma-no-findmany-without-take oxc backend — flag `*.findMany(...)` calls
//! whose options object lacks a `take:` or `first:` key.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference, ObjectPropertyKind};
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

/// Depth cap on spread-identifier resolution. A same-file binding can spread a
/// reference to itself (`const a = { ...a }`) or form a cycle across bindings;
/// resolving those would recurse forever, so a spread chain deeper than this is
/// treated conservatively as possibly-bounded (never flagged) rather than
/// followed further.
const MAX_SPREAD_DEPTH: u8 = 8;

/// Whether the options object bounds the result set. A literal `take:`/`first:`
/// key bounds it. An object-spread (`...opts`) may also carry `take`/`first`/
/// `cursor`, so it is resolved structurally: a spread of an inline object literal
/// (or of a same-file binding initialised from one) is recursed into precisely; a
/// spread whose argument is opaque to single-file analysis (a function-call
/// result, a cross-file import, a member access) is treated as possibly-bounded,
/// matching the rule's precision-first philosophy.
fn object_is_bounded<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    depth: u8,
) -> bool {
    let Expression::ObjectExpression(obj) = expr else {
        return false;
    };
    obj.properties.iter().any(|prop| match prop {
        ObjectPropertyKind::ObjectProperty(p) => {
            p.key.name().is_some_and(|n| n == "take" || n == "first")
        }
        ObjectPropertyKind::SpreadProperty(spread) => match &spread.argument {
            Expression::ObjectExpression(_) => object_is_bounded(&spread.argument, semantic, depth),
            Expression::Identifier(ident) => {
                depth >= MAX_SPREAD_DEPTH
                    || resolve_local_object_init(ident, semantic)
                        .is_none_or(|init| object_is_bounded(init, semantic, depth + 1))
            }
            _ => true,
        },
    })
}

/// Resolve an identifier reference to a same-file `let`/`const`/`var` declarator
/// whose initializer is an object literal, returning that initializer. `None`
/// when the binding is unresolved (a cross-file import), is not a variable
/// declarator, has no initializer, or is initialised from anything other than an
/// object literal (e.g. a function-call result) — in every such case the spread
/// is opaque to single-file analysis and the caller treats it conservatively.
fn resolve_local_object_init<'a>(
    ident: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a Expression<'a>> {
    let scoping = semantic.scoping();
    let symbol_id = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())?;
    let decl_id = scoping.symbol_declaration(symbol_id);
    let AstKind::VariableDeclarator(decl) = semantic.nodes().kind(decl_id) else {
        return None;
    };
    let init = decl.init.as_ref()?;
    matches!(init, Expression::ObjectExpression(_)).then_some(init)
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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        // Only a Prisma model delegate (`<client>.<model>.findMany(...)`) is a
        // real query; a wrapper self-call like `this.findMany(...)` inherits its
        // `take` bound from the underlying delegate call and must not be flagged.
        if !crate::oxc_helpers::is_prisma_delegate_call(member) {
            return;
        }

        // Bounded when any object argument carries `take:` or `first:`.
        let bounded = call
            .arguments
            .iter()
            .filter_map(|arg| arg.as_expression())
            .any(|arg| object_is_bounded(arg, semantic, 0));
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

    // Regression for #7807: `this.findMany(...)` is an inherited base-service
    // wrapper method, not a `<client>.<model>.findMany` delegate call — its
    // receiver is `this`, not a model accessor — so it must not be flagged.
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
    // (`this.prisma.<model>.findMany`) is still flagged when unbounded.
    #[test]
    fn flags_this_prisma_delegate_findmany() {
        let src = "import { PrismaClient } from '@prisma/client';\nexport class Repo { async load() { return this.prisma.user.findMany({ where: { active: true } }); } }";
        assert_eq!(run(src).len(), 1);
    }

    // `tx.user.findMany(...)` (transaction-client delegate) is still flagged.
    #[test]
    fn flags_transaction_delegate_findmany() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nawait prisma.$transaction(async (tx) => { await tx.user.findMany({ where: { active: true } }); });";
        assert_eq!(run(src).len(), 1);
    }

    // `prisma["user"].findMany(...)` is a delegate call (computed model
    // accessor) and still flagged when unbounded.
    #[test]
    fn flags_computed_delegate_findmany() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma[\"user\"].findMany({ where: { active: true } });";
        assert_eq!(run(src).len(), 1);
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
    fn allows_findmany_with_take_via_spread_of_call_result() {
        // Issue #7721: `take` is supplied through an object-spread of a value bound
        // to a function-call result (`buildPaginationQuery(...)` returns a
        // `PaginationQuery { take: number }`). The bound lives cross-file, so the
        // spread is opaque single-file and must be treated as possibly-bounded.
        let src = r#"import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();

export async function getCustomers(filters: Filters) {
  const paginationQuery = buildPaginationQuery(filters);
  return prisma.customer.findMany({
    where: { active: true },
    ...paginationQuery,
  });
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_findmany_with_take_via_spread_of_object_literal() {
        // A spread of an inline object literal carrying `take:` is resolved
        // precisely and recognised as bounded.
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ where: { active: true }, ...{ take: 10 } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_findmany_with_take_via_spread_of_local_binding() {
        // A spread of a same-file binding initialised from an object literal with
        // `take:` is resolved precisely and recognised as bounded.
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst opts = { take: 25 };\nconst rows = await prisma.user.findMany({ where: { active: true }, ...opts });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_findmany_with_spread_of_local_literal_without_take() {
        // Negative-space guard: a spread of a same-file object literal that
        // provably carries no `take`/`first` does not suppress the diagnostic.
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst opts = { active: true };\nconst rows = await prisma.user.findMany({ where: { active: true }, ...opts });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_findmany_with_spread_of_inline_literal_without_take() {
        // Negative-space guard: a spread of an inline object literal that provably
        // carries no `take`/`first` is resolved precisely and still flagged.
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst rows = await prisma.user.findMany({ ...{ active: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn cyclic_spread_binding_terminates() {
        // A cyclic same-file spread chain must not recurse forever; the depth cap
        // bails to the conservative (possibly-bounded) outcome without hanging.
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nconst a = { ...b };\nconst b = { ...a };\nconst rows = await prisma.user.findMany({ ...a });";
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
