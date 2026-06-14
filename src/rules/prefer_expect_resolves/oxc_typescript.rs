//! prefer-expect-resolves OXC backend — flag `expect(await promise)` calls.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};

pub struct Check;

/// True when the file imports `expect` (or anything) from the `chai` package.
/// Chai's `expect()` is a synchronous assertion wrapper with no `.resolves`
/// property, so the `await expect(promise).resolves` rewrite this rule
/// suggests is jest/vitest-only — applying it to a chai assertion would throw
/// a `TypeError` at runtime. The `expect` identifier this rule keys on means
/// chai's `expect` whenever the file imports from chai. Matches `'chai'` and
/// chai subpath specifiers (e.g. `'chai/register-expect'`).
fn imports_chai(program: &oxc_ast::ast::Program<'_>) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else {
            return false;
        };
        let source = import.source.value.as_str();
        source == "chai" || source.starts_with("chai/")
    })
}

/// Matchers attached to the `expect()` return value rather than the `.resolves`
/// proxy. These are snapshot/screenshot assertions (Playwright, jest-image-snapshot)
/// that read the already-resolved value synchronously; the `.resolves` proxy does
/// not forward them, so rewriting `expect(await p).toMatchSnapshot()` to
/// `await expect(p).resolves.toMatchSnapshot()` breaks at runtime. The core
/// vitest/jest value matchers that ARE proxied (`toMatchObject`, `toEqual`,
/// `toHaveLength`, `toHaveProperty`, …) are deliberately absent — those stay flagged.
const NON_RESOLVES_MATCHERS: &[&str] = &[
    "toMatchSnapshot",
    "toMatchInlineSnapshot",
    "toMatchAriaSnapshot",
    "toHaveScreenshot",
    "toMatchImageSnapshot",
];

/// The matcher name chained directly after `expect(...)`, i.e. the `m` in
/// `expect(await p).m(...)`. `None` when `expect(...)` is not the object of a
/// static member access. A leading `.not` modifier is transparent:
/// `expect(...).not.m()` resolves to `m`.
fn chained_matcher_name<'a>(
    call_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    let nodes = semantic.nodes();
    let parent = nodes.parent_node(call_id);
    let AstKind::StaticMemberExpression(member) = parent.kind() else {
        return None;
    };
    let name = member.property.name.as_str();
    if name != "not" {
        return Some(name);
    }
    // `expect(...).not.m()` — step over the `.not` modifier to the real matcher.
    let AstKind::StaticMemberExpression(inner) = nodes.parent_node(parent.id()).kind() else {
        return None;
    };
    Some(inner.property.name.as_str())
}

/// True when the awaited expression is a React-Testing-Library `findBy*` /
/// `findAllBy*` query. Those reject (throw) on not-found with RTL's own
/// diagnostic message, so `expect(await screen.findByText(...))` already fails
/// helpfully — rewriting to `.resolves` is no improvement and breaks the
/// canonical RTL idiom.
fn awaited_is_rtl_find_query(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr.without_parentheses() else {
        return false;
    };
    let name = match &call.callee {
        Expression::StaticMemberExpression(m) => m.property.name.as_str(),
        Expression::Identifier(id) => id.name.as_str(),
        _ => return false,
    };
    ["findBy", "findAllBy"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_ascii_uppercase())
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be the identifier `expect`.
        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "expect" {
            return;
        }

        // Must have exactly one argument, and it must be an await expression.
        if call.arguments.len() != 1 {
            return;
        }
        let Argument::AwaitExpression(await_expr) = &call.arguments[0] else { return };

        // RTL `findBy*` queries already reject on miss — no `.resolves` gain.
        if awaited_is_rtl_find_query(&await_expr.argument) {
            return;
        }

        // chai's `expect()` has no `.resolves` API — the suggested rewrite is
        // jest/vitest-only and would throw at runtime in a chai file.
        if imports_chai(semantic.nodes().program()) {
            return;
        }

        // Snapshot/screenshot matchers live on the `expect()` return value, not
        // the `.resolves` proxy — the rewrite would call an undefined method.
        if chained_matcher_name(node.id(), semantic)
            .is_some_and(|m| NON_RESOLVES_MATCHERS.contains(&m))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `await expect(promise).resolves` instead of `expect(await promise)`.".into(),
            severity: Severity::Warning,
            span: Some((call.span.start as usize, (call.span.end - call.span.start) as usize)),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_expect_await_value() {
        assert_eq!(run("expect(await getValue()).toEqual(1);").len(), 1);
    }

    // Regression for #270: RTL `findBy*`/`findAllBy*` queries reject on miss,
    // so `expect(await screen.findByText(...))` is the canonical idiom.
    #[test]
    fn skips_rtl_find_by_query() {
        let src = r#"expect(await screen.findByText("Mot de passe trop court.")).toBeInTheDocument();"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn skips_rtl_find_all_by_query() {
        let src = r#"expect(await screen.findAllByRole("button")).toHaveLength(2);"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn skips_bare_find_by_import() {
        assert!(run(r#"expect(await findByTestId("x")).toBeVisible();"#).is_empty());
    }

    // Regression for #1728: graphql-js (node:test + chai) imports `expect` from
    // chai, whose assertion object has no `.resolves` property. The suggested
    // rewrite would throw a TypeError, so the rule must not fire.
    #[test]
    fn skips_when_expect_imported_from_chai() {
        let src = r#"
import { expect } from 'chai';
const iterator = events[Symbol.asyncIterator]();
expect(await iterator.next()).to.deep.equal({ value: [], done: false });
"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn skips_when_chai_imported_via_subpath() {
        let src = r#"
import 'chai/register-expect';
expect(await getValue()).to.equal(1);
"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative space: a genuine jest/vitest `expect(await p)` (no chai import)
    // is still flagged — the chai guard must not over-suppress.
    #[test]
    fn flags_jest_expect_when_no_chai_import() {
        let src = r#"
import { expect } from 'vitest';
expect(await getValue()).toEqual(1);
"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #2264: Playwright snapshot/screenshot matchers attach to the
    // `expect()` return value, not the `.resolves` proxy, so the rewrite breaks.
    #[test]
    fn skips_to_match_snapshot() {
        let src = r#"expect(await page.screenshot()).toMatchSnapshot("screenshot-iframe.png");"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn skips_to_match_aria_snapshot() {
        let src = r#"expect(await page.locator("body")).toMatchAriaSnapshot();"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn skips_non_resolves_matcher_behind_not() {
        let src = r#"expect(await page.screenshot()).not.toMatchSnapshot("x.png");"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative space: core matchers ARE forwarded by the `.resolves` proxy, so
    // `expect(await p)` must STILL be flagged for them — the allowlist is exact.
    #[test]
    fn flags_to_be() {
        assert_eq!(run("expect(await p()).toBe(1);").len(), 1);
    }

    #[test]
    fn flags_to_match_object() {
        assert_eq!(run("expect(await p()).toMatchObject({ a: 1 });").len(), 1);
    }

    #[test]
    fn flags_to_equal() {
        assert_eq!(run("expect(await p()).toEqual({ a: 1 });").len(), 1);
    }
}
