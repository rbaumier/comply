//! ts-no-unused-vars OxcCheck backend — accurate unused-symbol detection via
//! oxc_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use std::sync::Arc;

#[derive(Debug)]
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
            let name = scoping.symbol_name(symbol_id);
            if name.starts_with('_') || name.is_empty() {
                continue;
            }
            if scoping.get_resolved_references(symbol_id).next().is_some() {
                continue;
            }

            let decl_node = scoping.symbol_declaration(symbol_id);
            let exported = nodes.ancestor_kinds(decl_node).any(|k| {
                matches!(
                    k,
                    AstKind::ExportNamedDeclaration(_)
                        | AstKind::ExportDefaultDeclaration(_)
                        | AstKind::ExportAllDeclaration(_)
                )
            });
            if exported {
                continue;
            }

            let decl_is_type_construct = matches!(
                nodes.kind(decl_node),
                AstKind::TSTypeAliasDeclaration(_) | AstKind::TSInterfaceDeclaration(_)
            );
            let in_type_decl = nodes.ancestor_kinds(decl_node).any(|k| {
                matches!(
                    k,
                    AstKind::TSTypeAliasDeclaration(_)
                        | AstKind::TSInterfaceDeclaration(_)
                        | AstKind::TSModuleDeclaration(_)
                        | AstKind::TSGlobalDeclaration(_)
                        | AstKind::TSFunctionType(_)
                ) || matches!(k, AstKind::Function(f) if f.declare)
            });
            if decl_is_type_construct || in_type_decl {
                continue;
            }

            let span = scoping.symbol_span(symbol_id);
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`{name}` is declared but never used."),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }

    #[test]
    fn no_fp_on_var_in_declare_global() {
        // `declare global` augments the global scope; its bindings are used by
        // consumers elsewhere and must not be reported as unused. (Closes #339)
        assert!(
            run("declare global {\n  var BASE_UI_ANIMATIONS_DISABLED: boolean;\n}\nexport {};")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_unused_local() {
        assert_eq!(run("const unusedThing = 1;\nexport {};").len(), 1);
    }

    #[test]
    fn no_fp_on_declare_function_params() {
        // Params of a declare function have no runtime body → zero runtime
        // references, but are semantically required for the type signature.
        let src = r#"
declare function replace<
    Input extends string,
    Search extends string,
>(
    input: Input,
    search: Search,
): string;
export {};
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`input`")),
            "FP on `input` param of declare function"
        );
        assert!(
            !diags.iter().any(|d| d.message.contains("`search`")),
            "FP on `search` param of declare function"
        );
    }

    #[test]
    fn no_fp_on_type_assertion_alias() {
        // `type t = Expect<Equal<...>>` is the standard type-assertion idiom
        // in test-d/ files (type-fest, tsd). `t` is never referenced at runtime
        // — that is intentional.
        let src = r#"
type Expect<T> = T extends true ? true : never;
type Equal<X, Y> = X extends Y ? (Y extends X ? true : false) : false;
const result = {};
type t = Expect<Equal<typeof result, {}>>;
export {};
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`t`")),
            "FP on type assertion alias `t`"
        );
    }

    #[test]
    fn no_fp_on_unused_interface_name() {
        // The declaration node for `Foo` is the TSInterfaceDeclaration itself;
        // without decl_is_type_construct, `Foo` would be flagged as unused.
        let src = r#"
interface Foo {
    bar: string;
}
export {};
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`Foo`")),
            "FP on interface name `Foo`"
        );
    }




    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_unused_variable() {
        let d = run_on("const unused = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`unused`"));
    }


    #[test]
    fn allows_used_variable() {
        assert!(run_on("const x = 1; console.log(x);").is_empty());
    }


    #[test]
    fn allows_underscore_prefix() {
        assert!(run_on("const _unused = 42;").is_empty());
    }


    #[test]
    fn allows_exported_variable() {
        assert!(run_on("export const foo = 42;").is_empty());
    }


    #[test]
    fn flags_multiple_unused() {
        let d = run_on("const aaa = 1; const bbb = 2;");
        assert_eq!(d.len(), 2);
    }


    #[test]
    fn flags_unused_destructured_binding() {
        let d = run_on("const obj = { a: 1, b: 2 }; const { a, b } = obj; console.log(a);");
        assert_eq!(d.len(), 1, "destructured `b` is unused");
        assert!(d[0].message.contains("`b`"));
    }


    #[test]
    fn allows_shared_name_with_outer_use() {
        let d = run_on("const x = 1; function f(x: number) { return x; } f(2); console.log(x);");
        assert!(
            d.is_empty(),
            "param `x` is used in body, outer `x` is logged"
        );
    }


    #[test]
    fn flags_unused_import() {
        let d = run_on("import { foo } from './x'; console.log('hello');");
        assert_eq!(d.len(), 1, "imported `foo` is never used");
        assert!(d[0].message.contains("`foo`"));
    }


    #[test]
    fn skips_params_in_type_alias() {
        let src = r#"
type JsonReplacer = (key: string, value: unknown) => unknown;
export const x: JsonReplacer = (k, v) => v;
"#;
        let d = run_on(src);
        assert!(
            !d.iter().any(|d| d.message.contains("`key`") || d.message.contains("`value`")),
            "params in type signatures are not runtime vars"
        );
    }


    #[test]
    fn skips_params_in_interface_method() {
        let src = r#"
interface Store {
  subscribe(listener: () => void): () => void;
}
export function createStore(): Store { return null as any; }
"#;
        let d = run_on(src);
        assert!(
            !d.iter().any(|d| d.message.contains("`listener`")),
            "interface method params are not runtime vars"
        );
    }


    #[test]
    fn skips_type_params_in_declare_module() {
        let src = r#"
declare module '../vanilla' {
  interface StoreMutators<S, A> {
    ['test']: S;
  }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
