//! ts-no-redeclare OXC backend — detect duplicate variable declarations
//! via oxc_semantic symbol model.
//!
//! Function and method overload signatures share an identifier with
//! their implementation by language design — skipped when every
//! declaration of a symbol is a `Function` AST node.
//!
//! Branded type pattern: TypeScript allows a symbol to occupy both the
//! value namespace (`const`/function/import binding) and the type namespace
//! (`type`/`interface`) simultaneously — skipped when declarations are a mix
//! of value and type-only nodes (e.g. `import Database from 'better-sqlite3'`
//! + `export interface Database`). A type-only import specifier
//! (`import type { X }` or `import { type X }`) binds `X` in the type
//! namespace only, so it counts as a type declaration for this merge (a
//! function `X` coexisting with `import type { X }` is the standard
//! component-named-after-its-props-type pattern).
//!
//! Generic type parameters live in the type namespace, so a type parameter
//! sharing a name with a value declaration (`<t>(t: t)`) is legal —
//! skipped when any declaration of a symbol is a `TSTypeParameter` node.
//! `infer` type variables (`T extends ... infer U ...`) surface as the same
//! `TSTypeParameter` node (parented by a `TSInferType`), so reusing one `infer`
//! name across the arms of a union conditional type is covered by this skip too.
//!
//! Namespace declaration merging: a `namespace`/`module`
//! (`TSModuleDeclaration`) may share a name with an interface, class,
//! function, or enum (e.g. `interface Foo` + `declare namespace Foo`,
//! the Standard Schema V1 pattern) — skipped when one declaration is a
//! namespace and another is a type or value declaration.
//!
//! `@ts-expect-error` opt-in: a redeclaration immediately preceded by a
//! `// @ts-expect-error` (or `/* @ts-expect-error */`) comment is deliberate —
//! TypeScript already reports it and the author opted in. Idiomatic in tsd
//! `test-d/` files that reuse short test-local type names across fixtures.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, has_ts_expect_error_above};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::VariableDeclarationKind;
use oxc_ast::AstKind;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True for Angular compiler-compliance golden fixtures: a `GOLDEN_PARTIAL.js`
/// file whose body concatenates several independently-generated compiler
/// outputs, each introduced by a `* PARTIAL FILE:` comment delimiter. Every
/// section re-declares its own `MyApp`, `Component`, etc. by design, so the
/// cross-section "redeclaration" is an artifact of concatenation, not a bug.
fn is_concatenated_golden_partial(ctx: &CheckCtx) -> bool {
    ctx.path.file_name().and_then(|n| n.to_str()) == Some("GOLDEN_PARTIAL.js")
        && ctx.source_contains("PARTIAL FILE:")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if is_concatenated_golden_partial(ctx) {
            return Vec::new();
        }

        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let decl_ids: Vec<_> = scoping.symbol_declarations(symbol_id).collect();
            if decl_ids.len() <= 1 {
                continue;
            }

            let all_functions = decl_ids
                .iter()
                .all(|&id| matches!(nodes.kind(id), AstKind::Function(_)));
            if all_functions {
                continue;
            }

            // Type-only import specifier: `import type { X }` (statement-level)
            // or `import { type X }` (specifier-level) binds `X` in the type
            // namespace only, with no runtime presence — so it merges with a
            // value declaration of the same name exactly like a type alias does.
            let is_type_only_import = |id| -> bool {
                let AstKind::ImportSpecifier(spec) = nodes.kind(id) else {
                    return false;
                };
                spec.import_kind.is_type()
                    || matches!(
                        nodes.kind(nodes.parent_id(id)),
                        AstKind::ImportDeclaration(import) if import.import_kind.is_type()
                    )
            };

            // Branded type pattern: `export const Foo = ...; export type Foo = ...;`
            // TypeScript merges value-namespace (const/function/import binding)
            // and type-namespace (type alias/interface) declarations — exempt only
            // when both sides present and every decl is one or the other
            // (const only, not let/var).
            let is_value_decl = |id| -> bool {
                match nodes.kind(id) {
                    AstKind::Function(_)
                    | AstKind::ImportDefaultSpecifier(_)
                    | AstKind::ImportNamespaceSpecifier(_) => true,
                    AstKind::ImportSpecifier(_) => !is_type_only_import(id),
                    AstKind::VariableDeclarator(_) => {
                        let parent_id = nodes.parent_id(id);
                        matches!(nodes.kind(parent_id), AstKind::VariableDeclaration(d) if d.kind == VariableDeclarationKind::Const)
                    }
                    _ => false,
                }
            };
            let is_type_decl = |id| -> bool {
                matches!(
                    nodes.kind(id),
                    AstKind::TSTypeAliasDeclaration(_) | AstKind::TSInterfaceDeclaration(_)
                ) || is_type_only_import(id)
            };
            if decl_ids.iter().all(|&id| is_value_decl(id) || is_type_decl(id))
                && decl_ids.iter().any(|&id| is_value_decl(id))
                && decl_ids.iter().any(|&id| is_type_decl(id))
            {
                continue;
            }

            // Namespace declaration merging: a `namespace`/`module` may share a
            // name with an interface, class, function, or enum (`interface Foo`
            // + `declare namespace Foo`, the Standard Schema V1 pattern). Always
            // intentional in TypeScript.
            let is_namespace =
                |id| -> bool { matches!(nodes.kind(id), AstKind::TSModuleDeclaration(_)) };
            if decl_ids.iter().any(|&id| is_namespace(id))
                && decl_ids
                    .iter()
                    .any(|&id| is_type_decl(id) || is_value_decl(id))
            {
                continue;
            }

            // Generic type parameter sharing a name with a value declaration
            // (e.g. `<input extends object>(input: input) => input`): the type
            // parameter lives in the type namespace, so coexisting with a
            // value-namespace name is always legal. `infer U` bindings also
            // surface as `TSTypeParameter` nodes, so reusing one `infer` name
            // across the arms of a union conditional type is covered here too.
            let is_type_param =
                |id| -> bool { matches!(nodes.kind(id), AstKind::TSTypeParameter(_)) };
            if decl_ids.iter().any(|&id| is_type_param(id)) {
                continue;
            }

            let name = scoping.symbol_name(symbol_id);
            for &decl_id in &decl_ids[1..] {
                let span = nodes.kind(decl_id).span();
                // A redeclaration the author marked `@ts-expect-error` is
                // intentional — TypeScript already reports it and the author has
                // opted in (idiomatic in tsd `test-d/` files that reuse short
                // test-local names across fixtures).
                if has_ts_expect_error_above(
                    semantic.comments(),
                    ctx.source,
                    span.start as usize,
                ) {
                    continue;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "ts-no-redeclare".into(),
                    message: format!("`{name}` is already defined."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_duplicate_var() {
        let d = run("var x = 1; var x = 2;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_different_scopes() {
        let d = run("function a() { let x = 1; } function b() { let x = 2; }");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_function_overloads() {
        let d = run(
            "function foo(a: string): string;\nfunction foo(a: number): number;\nfunction foo(a: any): any { return a; }",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_duplicate_function_declarations() {
        // Two function declarations = valid TS overload pattern
        let d = run("function foo() {} function foo() {}");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_branded_type_unique_symbol() {
        // Regression for #809: const + type alias with same name is a branded type pattern
        let d = run(
            "export const UserId: unique symbol = Symbol(\"UserId\");\nexport type UserId = typeof UserId;",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_const_plus_type_alias() {
        // Regression for #809: plain const + type alias (zod/fp-ts pattern)
        let d = run(
            "export const Brand1 = Symbol(\"Brand1\");\nexport type Brand1 = typeof Brand1;",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn flags_duplicate_const() {
        // Two const declarations = real redeclaration
        let d = run("export const x = 1;\nexport const x = 2;");
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn flags_duplicate_let() {
        // Two let declarations = real redeclaration
        let d = run("let y = 1;\nlet y = 2;");
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`y`"));
    }

    #[test]
    fn flags_let_plus_type_alias() {
        // let + type alias is NOT the branded type pattern (const only)
        let d = run("let Foo = 1;\ntype Foo = string;");
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`Foo`"));
    }

    #[test]
    fn allows_exported_generic_overloads() {
        // Regression for #124: overloads with const type parameters and
        // intersection-typed options must not trigger ts-no-redeclare.
        let src = r#"
import { z } from "zod";

type SortColumns = readonly [string, ...string[]];
type SortFor<T extends SortColumns> = `${T[number]}:asc` | `${T[number]}:desc`;

type FilterMap = Record<string, z.ZodType<unknown>>;
type NoFiltersOptions<C extends SortColumns> = {
  sortColumns: C;
  defaultSort: SortFor<C>;
};
type WithFiltersOptions<F extends FilterMap, C extends SortColumns> =
  NoFiltersOptions<C> & { filters: F };

export function make<const C extends SortColumns>(
  opts: NoFiltersOptions<C>,
): z.ZodObject<{ sort: z.ZodTransform<SortFor<C>, unknown> }>;
export function make<F extends FilterMap, const C extends SortColumns>(
  opts: WithFiltersOptions<F, C>,
): z.ZodObject<F & { sort: z.ZodTransform<SortFor<C>, unknown> }>;
export function make<F extends FilterMap, const C extends SortColumns>(
  opts: NoFiltersOptions<C> | WithFiltersOptions<F, C>,
) {
  return opts as any;
}
"#;
        let d = run(src);
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_type_param_sharing_name_with_value_param() {
        // Regression for #967: generic type parameter and value parameter
        // sharing a name (`<s>(s: s)`) occupy distinct namespaces.
        let d = run(
            "const capitalize = <s extends string>(s: s): Capitalize<s> =>\n  (s[0].toUpperCase() + s.slice(1)) as never",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_type_param_in_function_type_annotation() {
        // Regression for #967: arktype's shallowClone — type param `input` in
        // the annotation + value param `input` in the implementation.
        let d = run(
            "declare const _clone: (v: object, x: null) => object;\nexport const shallowClone: <input extends object>(\n  input: input\n) => input = input => _clone(input, null) as never;",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn flags_let_plus_var_redeclaration() {
        // Genuine value-namespace redeclaration with no type parameter
        // involved must still fire.
        let d = run("let x = 1;\nvar x = 2;");
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_interface_plus_namespace_merging() {
        // Regression for #969: interface + namespace declaration merging
        // (Standard Schema V1 pattern, used by zod/valibot/arktype).
        let d = run(
            "export interface StandardSchemaV1<Input = unknown, Output = Input> {\n  readonly \"~standard\": StandardSchemaV1.Props<Input, Output>;\n}\nexport declare namespace StandardSchemaV1 {\n  export interface Props<Input = unknown, Output = Input> {}\n  export type InferInput<Schema extends StandardSchemaV1> = unknown;\n}",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_default_import_plus_interface() {
        // Regression for #970: a value import + an interface of the same name
        // occupy distinct namespaces (kysely test-setup pattern).
        let d = run(
            "import Database from 'better-sqlite3'\nexport interface Database {\n  person: Person\n  pet: Pet\n  toy: Toy\n}",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_named_import_plus_type_alias() {
        // Regression for #970: named import + type alias of the same name.
        let d = run("import { Foo } from 'x'\ntype Foo = { a: string }");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn flags_import_plus_const_redeclaration() {
        // import + const of the same name are both value-namespace
        // declarations — a genuine redeclaration, still flagged.
        let d = run("import Foo from 'a'\nconst Foo = 1;");
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`Foo`"));
    }

    #[test]
    fn allows_type_only_import_plus_function() {
        // Regression for #1804: vercel/commerce's filter/item.tsx declares
        // React components named after their props types, which arrive via
        // `import type`. A type-only import binds the name in the type
        // namespace, so it merges with a value-namespace function.
        let src = r#"import type { SortFilterItem } from "lib/constants";
import type { PathFilterItem } from ".";

function PathFilterItem({ item }: { item: PathFilterItem }) {
  return null;
}

function SortFilterItem({ item }: { item: SortFilterItem }) {
  return null;
}"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "item.tsx");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_inline_type_import_specifier_plus_function() {
        // Specifier-level `import { type X }` also binds in the type namespace.
        let d = run("import { type Foo } from 'x';\nfunction Foo() { return null; }");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn flags_value_import_plus_function() {
        // A plain (value) named import + a function of the same name are both
        // value-namespace declarations — a genuine redeclaration, still flagged.
        let d = run("import { Foo } from 'x';\nfunction Foo() { return null; }");
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`Foo`"));
    }

    #[test]
    fn allows_function_plus_namespace_merging() {
        // Regression for #969: function + namespace declaration merging is
        // also intentional TypeScript.
        let d = run("function fn() {}\nnamespace fn {\n  export const x = 1;\n}");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_infer_in_union_conditional_arms() {
        // Regression for #1122: each arm of a union of conditional-type extends
        // clauses independently introduces `infer TPage`; together they form
        // union-branch inference. `infer` type variables are not lexical
        // redeclarations and must not be flagged.
        let d = run(
            "export type PaginateReturn<TResult> = TResult extends\n  | { body: { value?: infer TPage } }\n  | { body: { Value?: infer TPage } }\n  ? GetArrayType<TPage> : Array<unknown>;",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_type_alias_redeclaration_under_ts_expect_error() {
        // Regression for #3323: type-fest's test-d files reuse short test-local
        // type names across fixtures, each redeclaration under
        // `// @ts-expect-error`. The author opted into the TS error — not a bug.
        let d = run(
            "// @ts-expect-error\ntype NoSplice = First['splice'];\n// @ts-expect-error\ntype NoSplice = Second['splice'];",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_type_alias_redeclaration_under_block_ts_expect_error() {
        // The opt-in applies to block-comment `/* @ts-expect-error */` form too.
        let d = run(
            "/* @ts-expect-error */\ntype Dup = First['x'];\n/* @ts-expect-error */\ntype Dup = Second['x'];",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn flags_type_alias_redeclaration_without_ts_expect_error() {
        // Guard: a genuine accidental redeclaration with no `@ts-expect-error`
        // marker must still fire.
        let d = run("type Dup = First['x'];\ntype Dup = Second['x'];");
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`Dup`"));
    }

    #[test]
    fn flags_redeclaration_when_ts_expect_error_only_on_first_decl() {
        // The marker exempts only the declaration it directly precedes. A
        // marker above the *first* declaration does not license a later
        // unmarked redeclaration.
        let d = run("// @ts-expect-error\ntype Dup = First['x'];\ntype Dup = Second['x'];");
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`Dup`"));
    }

    #[test]
    fn allows_single_infer_in_conditional() {
        // A single conditional type with one `infer T` used normally must not
        // be flagged.
        let d = run("type Unwrap<T> = T extends Promise<infer U> ? U : T;");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_concatenated_golden_partial_fixture() {
        // Regression for #1398: Angular's GOLDEN_PARTIAL.js compliance fixtures
        // concatenate independently-generated compiler outputs, each delimited
        // by a `* PARTIAL FILE:` comment. Re-declarations across sections are
        // an artifact of concatenation, not a real redeclaration.
        let src = "/*\n * PARTIAL FILE: switch_without_default.js\n */\nimport { Component } from '@angular/core';\nexport class MyApp {}\n/*\n * PARTIAL FILE: switch_with_default.js\n */\nimport { Component } from '@angular/core';\nexport class MyApp {}";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "GOLDEN_PARTIAL.js");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn flags_redeclaration_in_normal_file_named_golden_partial() {
        // The exemption is gated on the `PARTIAL FILE:` concatenation marker:
        // a genuine redeclaration in a file merely named GOLDEN_PARTIAL.js but
        // without the delimiter is still a real bug and must fire.
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "export const x = 1;\nexport const x = 2;",
            "GOLDEN_PARTIAL.js",
        );
        assert_eq!(d.len(), 1, "expected 1 diagnostic, got: {d:?}");
        assert!(d[0].message.contains("`x`"));
    }
}
