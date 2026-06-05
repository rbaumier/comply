//! ts-no-redeclare OXC backend — detect duplicate variable declarations
//! via oxc_semantic symbol model.
//!
//! Function and method overload signatures share an identifier with
//! their implementation by language design — skipped when every
//! declaration of a symbol is a `Function` AST node.
//!
//! Branded type pattern: TypeScript allows a symbol to occupy both the
//! value namespace (`const`/function) and the type namespace (`type`/`interface`)
//! simultaneously — skipped when declarations are a mix of value and type-only nodes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::VariableDeclarationKind;
use oxc_ast::AstKind;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
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

            // Branded type pattern: `export const Foo = ...; export type Foo = ...;`
            // TypeScript merges value-namespace (const/function) and type-namespace
            // (type alias/interface) declarations — exempt only when both sides present
            // and every decl is one or the other (const only, not let/var).
            let is_value_decl = |id| -> bool {
                match nodes.kind(id) {
                    AstKind::Function(_) => true,
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
                )
            };
            if decl_ids.iter().all(|&id| is_value_decl(id) || is_type_decl(id))
                && decl_ids.iter().any(|&id| is_value_decl(id))
                && decl_ids.iter().any(|&id| is_type_decl(id))
            {
                continue;
            }

            let name = scoping.symbol_name(symbol_id);
            for &decl_id in &decl_ids[1..] {
                let span = nodes.kind(decl_id).span();
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
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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
}
