//! OxcCheck backend — flag `<x>.$queryRaw(...)` and `<x>.$executeRaw(...)` call
//! forms that pass a raw string. The safe forms are the tagged template literal
//! `<x>.$queryRaw\`...\`` and passing a `Prisma.sql`/`Prisma.raw`/`Prisma.join`
//! builder (e.g. `<x>.$queryRaw(Prisma.sql\`... ${value}\`)`), whose
//! interpolations become bound parameters rather than concatenated SQL.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "$queryRaw")
        || crate::oxc_helpers::source_contains(source, "$executeRaw")
}

/// A `Prisma.<member>` static member expression (object identifier `Prisma`),
/// the tag/callee shared by the parameterized SQL builders. The `member` names
/// the builder (`sql`, `raw`, `join`, `empty`).
fn prisma_builder_member<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = expr else {
        return None;
    };
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    if obj.name.as_str() != "Prisma" {
        return None;
    }
    Some(member.property.name.as_str())
}

/// The argument is a parameterized Prisma SQL builder, so its interpolations are
/// bound parameters, not concatenated SQL. Recognizes both the tagged-template
/// builder `Prisma.sql\`...\`` and a direct `Prisma.sql(...)`/`Prisma.raw(...)`/
/// `Prisma.join(...)`/`Prisma.empty` builder result passed as the query.
fn is_safe_prisma_builder(expr: &Expression) -> bool {
    match expr {
        Expression::TaggedTemplateExpression(tagged) => {
            matches!(
                prisma_builder_member(&tagged.tag),
                Some("sql" | "raw" | "join" | "empty")
            )
        }
        Expression::CallExpression(call) => {
            matches!(
                prisma_builder_member(&call.callee),
                Some("sql" | "raw" | "join" | "empty")
            )
        }
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["$queryRaw", "$executeRaw"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_prisma_file(ctx.source) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop_text = member.property.name.as_str();
        if !matches!(prop_text, "$queryRaw" | "$executeRaw") {
            return;
        }

        // The tagged-template form `prisma.$queryRaw\`...\`` is parsed by oxc as
        // a TaggedTemplateExpression, not a CallExpression, so reaching here
        // means a call form. A zero-argument call (e.g. a type-assertion test)
        // passes no query, and a single argument that is a parameterized Prisma
        // SQL builder (`Prisma.sql\`...\``) binds its interpolations as
        // parameters — both are safe.
        match call.arguments.first().and_then(|arg| arg.as_expression()) {
            None => return,
            Some(arg) if is_safe_prisma_builder(arg) => return,
            Some(_) => {}
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{prop_text}(...)` accepts a string — concatenated input is SQL injection. \
                 Use the tagged-template form: `prisma.{prop_text}\\`SELECT ...\\``."
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
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    const PRELUDE: &str = "import { PrismaClient, Prisma } from '@prisma/client';\nconst prisma = new PrismaClient();\n";

    #[test]
    fn flags_string_concat_call_form() {
        let src = format!(
            "{PRELUDE}async function f(id: string) {{ return prisma.$queryRaw('SELECT * FROM u WHERE id = ' + id); }}"
        );
        assert_eq!(run(&src).len(), 1, "{:?}", run(&src));
    }

    // Untagged interpolated template passed as the argument is raw concatenation
    // (the values are not bound parameters) — must still flag.
    #[test]
    fn flags_untagged_interpolated_template_arg() {
        let src = format!(
            "{PRELUDE}async function f(id: number) {{ return prisma.$queryRaw(`SELECT * FROM u WHERE id = ${{id}}`); }}"
        );
        assert_eq!(run(&src).len(), 1, "{:?}", run(&src));
    }

    // Regression for #3350: `Prisma.sql\`...\`` is a parameterized builder whose
    // interpolations become bound parameters, so passing it is safe (the issue's
    // exact `send-type-hints/tests.ts:45` example).
    #[test]
    fn allows_prisma_sql_tagged_template_arg() {
        let src = format!(
            "{PRELUDE}async function f() {{ await prisma.$queryRaw(Prisma.sql`INSERT INTO Entry (id, binary) VALUES ('3', ${{Uint8Array.from([1, 2, 3])}})`); }}"
        );
        assert!(run(&src).is_empty(), "{:?}", run(&src));
    }

    // Regression for #3350: `Prisma.join` nested under `Prisma.sql` in an
    // `$executeRaw` tagged template, passed as a call argument.
    #[test]
    fn allows_prisma_sql_with_join_execute_raw() {
        let src = format!(
            "{PRELUDE}async function f() {{ const affected = await prisma.$executeRaw(Prisma.sql`UPDATE User SET age = ${{65}} WHERE age IN (${{Prisma.join([45, 60])}})`); return affected; }}"
        );
        assert!(run(&src).is_empty(), "{:?}", run(&src));
    }

    // Regression for #3350: a zero-argument call (TypeScript type-assertion
    // test) passes no query and has no injection risk.
    #[test]
    fn allows_zero_argument_call() {
        let src = format!("{PRELUDE}async function f() {{ return prisma.$queryRaw(); }}");
        assert!(run(&src).is_empty(), "{:?}", run(&src));
    }

    #[test]
    fn allows_direct_tagged_template_form() {
        let src = format!(
            "{PRELUDE}async function f(id: number) {{ return prisma.$queryRaw`SELECT * FROM u WHERE id = ${{id}}`; }}"
        );
        assert!(run(&src).is_empty(), "{:?}", run(&src));
    }
}
