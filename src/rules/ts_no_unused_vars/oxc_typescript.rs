//! ts-no-unused-vars OxcCheck backend — accurate unused-symbol detection via
//! oxc_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::FunctionType;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

/// True when `path` is a code-splitter / test snapshot file. Snapshot files are
/// machine-generated transformer output committed verbatim so the test can
/// compare against them. After code-splitting, each output chunk keeps only the
/// imports its own code needs, leaving the rest "unused" by design — flagging
/// them would report on artifacts no human edits.
fn is_snapshot_file(path: &std::path::Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized.split('/').any(|seg| seg == "snapshots" || seg == "__snapshots__")
}

/// True when `decl_node` is `import React from 'react'`. Under the classic JSX
/// transform (`jsx: "react"`), each JSX element compiles to a
/// `React.createElement(...)` call, so the `React` binding is consumed by the
/// JSX in the file even though it never appears as an explicit source
/// reference. oxc's semantic analysis only sees source references, so it
/// reports this pragma import as unused.
fn is_react_pragma_import(
    decl_node: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::ImportDefaultSpecifier(spec) = nodes.kind(decl_node) else {
        return false;
    };
    if spec.local.name.as_str() != "React" {
        return false;
    }
    nodes.ancestor_kinds(decl_node).any(
        |k| matches!(k, AstKind::ImportDeclaration(import) if import.source.value == "react"),
    )
}

/// True when `decl_node` is the id of a named function *expression*
/// (`const f = function name() {}`). That name is the function's own identity,
/// scoped to its body for self-reference / stack traces — it is intentionally
/// not a binding in the outer scope, so an absence of references is by design,
/// not dead code. A function *declaration* (`function name() {}`) is excluded:
/// its name is a real outer-scope binding and an unused one stays reportable.
fn is_named_function_expression_id(
    decl_node: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    matches!(
        semantic.nodes().kind(decl_node),
        AstKind::Function(func)
            if matches!(
                func.r#type,
                FunctionType::FunctionExpression | FunctionType::TSEmptyBodyFunctionExpression
            )
    )
}

/// True when the program contains any JSX element or fragment.
fn file_contains_jsx(semantic: &oxc_semantic::Semantic) -> bool {
    semantic
        .nodes()
        .iter()
        .any(|node| matches!(node.kind(), AstKind::JSXElement(_) | AstKind::JSXFragment(_)))
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
        if is_snapshot_file(ctx.path) {
            return Vec::new();
        }
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();
        // Memoized once a React pragma import is encountered; scanning every
        // node for JSX is only worth paying for when there's a React import.
        let mut has_jsx: Option<bool> = None;

        for symbol_id in scoping.symbol_ids() {
            let name = scoping.symbol_name(symbol_id);
            if name.starts_with('_') || name.is_empty() {
                continue;
            }
            if scoping.get_resolved_references(symbol_id).next().is_some() {
                continue;
            }

            let decl_node = scoping.symbol_declaration(symbol_id);

            if is_named_function_expression_id(decl_node, semantic) {
                continue;
            }

            if is_react_pragma_import(decl_node, semantic)
                && *has_jsx.get_or_insert_with(|| file_contains_jsx(semantic))
            {
                continue;
            }
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

            // A mapped type's key (`k` in `{[k in K]: T}`) is declared on the
            // `TSMappedType` node itself, so it is the declaration node — not an
            // ancestor of it. It is a type-level loop variable with no runtime
            // binding, so it belongs alongside the other type-construct decls.
            let decl_is_type_construct = matches!(
                nodes.kind(decl_node),
                AstKind::TSTypeAliasDeclaration(_)
                    | AstKind::TSInterfaceDeclaration(_)
                    | AstKind::TSMappedType(_)
            );
            let in_type_decl = nodes.ancestor_kinds(decl_node).any(|k| {
                matches!(
                    k,
                    AstKind::TSTypeAliasDeclaration(_)
                        | AstKind::TSInterfaceDeclaration(_)
                        | AstKind::TSModuleDeclaration(_)
                        | AstKind::TSGlobalDeclaration(_)
                        | AstKind::TSFunctionType(_)
                        | AstKind::TSMappedType(_)
                ) || matches!(k, AstKind::Function(f) if f.body.is_none())
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, path)
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

    #[test]
    fn no_fp_on_code_splitter_snapshot() {
        // Code-splitter snapshots are machine-generated transformer output: each
        // emitted chunk keeps only the imports its own code needs, so the rest
        // are "unused" by design. (Closes #1416)
        let src = r#"
import { Await, Link } from '@tanstack/react-router';
import { twMerge } from 'tailwind-merge';
import { Route } from "random-number.tsx";
function Index() {}
export { Index as component };
"#;
        let path = "packages/router-plugin/tests/code-splitter/snapshots/random-number@component.tsx";
        assert!(
            run_at(src, path).is_empty(),
            "FP on unused imports in a code-splitter snapshot file"
        );
    }

    #[test]
    fn still_flags_unused_import_in_ordinary_source() {
        // True-positive guard: an unused import in a hand-written source file
        // (outside any snapshot directory) must still fire.
        let src = "import { twMerge } from 'tailwind-merge';\nexport {};";
        assert_eq!(run_at(src, "src/index.tsx").len(), 1);
    }

    #[test]
    fn no_fp_on_overload_signature_params() {
        // Overload signatures are function declarations with no body: their
        // parameter names are documentary only, never referenced at runtime.
        // Only the implementation signature's body can reference params.
        // (Closes #1853)
        let src = r#"
function customElement<T extends object>(
    tag: string,
    ComponentType: ComponentType<T>,
): CustomElementConstructor;
function customElement<T extends object>(
    tag: string,
    props: PropsDefinitionInput<T>,
    ComponentType: ComponentType<T>,
): CustomElementConstructor;
function customElement<T extends object>(
    tag: string,
    props: PropsDefinitionInput<T> | ComponentType<T>,
    ComponentType?: ComponentType<T>,
): CustomElementConstructor {
    return tag.length + props + ComponentType as unknown as CustomElementConstructor;
}
export { customElement };
"#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "FP on overload signature parameter names: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_react_default_import_with_jsx() {
        // Classic JSX transform (`jsx: "react"`): every JSX element compiles to
        // `React.createElement(...)`, so the `React` default import is used by
        // the JSX even though it never appears as an explicit reference.
        // (Closes #1864)
        let src = r#"
import React from 'react';

export const CustomizedDot = {
    render: () => (
        <svg x={1} y={2} width={20} height={20}>
            <path d="M0 0" />
        </svg>
    ),
};
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`React`")),
            "FP on `React` default import in a .tsx file containing JSX: {diags:?}"
        );
    }

    #[test]
    fn still_flags_react_default_import_without_jsx() {
        // No JSX in the file → the classic transform consumes nothing, so an
        // unused `React` default import is genuinely dead code.
        let src = "import React from 'react';\nexport {};";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected `React` to be flagged: {diags:?}");
        assert!(diags[0].message.contains("`React`"));
    }

    #[test]
    fn still_flags_unused_non_react_default_import_with_jsx() {
        // The carve-out is scoped to the `React` pragma binding from `react`.
        // An unrelated unused default import is still dead code, even when the
        // file contains JSX.
        let src = r#"
import React from 'react';
import unusedDefault from './other';

export const El = () => <div />;
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`React`")),
            "FP on `React` default import: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`unusedDefault`")),
            "expected unused non-React default import to be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_named_function_expression_id() {
        // The inner name of a named function expression is scoped to the
        // function body (self-reference / stack traces) and is intentionally not
        // a binding in the outer scope, so its absence of outer references is by
        // design, not dead code. (Closes #1603)
        let src = r#"
const machineSnapshotMatches = function matches(
    this: AnyMachineSnapshot,
    testValue: StateValue,
) {
    return matchesState(testValue, this.value);
};

const machineSnapshotHasTag = function hasTag(
    this: AnyMachineSnapshot,
    tag: string,
) {
    return this.tags.has(tag);
};
export { machineSnapshotMatches, machineSnapshotHasTag };
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`matches`")),
            "FP on named function expression id `matches`: {diags:?}"
        );
        assert!(
            !diags.iter().any(|d| d.message.contains("`hasTag`")),
            "FP on named function expression id `hasTag`: {diags:?}"
        );
    }

    #[test]
    fn still_flags_unused_function_declaration() {
        // Negative-space guard: a function *declaration*'s name is a real
        // outer-scope binding, so a genuinely unused one is still dead code.
        let src = "function unusedHelper() {}\nexport {};";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected `unusedHelper` to be flagged: {diags:?}");
        assert!(diags[0].message.contains("`unusedHelper`"));
    }

    #[test]
    fn no_fp_on_mapped_type_key_in_type_predicate() {
        // `k` in `{[k in K]: unknown}` is a mapped-type key parameter — a
        // type-level loop variable that never has a runtime binding. Inside a
        // type predicate its ancestor chain passes through TSMappedType, not the
        // other type wrappers. (Closes #1888)
        let src = r#"
function hasProperty<T extends object, K extends PropertyKey>(
    obj: T,
    prop: K,
): obj is T & {[k in K]: unknown} {
    return prop in obj
}
export { hasProperty };
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`k`")),
            "FP on mapped-type key parameter `k`"
        );
    }

}
