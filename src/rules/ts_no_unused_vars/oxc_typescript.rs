//! ts-no-unused-vars OxcCheck backend — accurate unused-symbol detection via
//! oxc_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_custom_element_decorator_name};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, FunctionType, TSTypeName};
use oxc_semantic::SymbolId;
use std::collections::HashSet;
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

/// True when the binding spanning `symbol_span` is a non-rest property of an
/// object pattern that also has a rest element (`const { a, b, ...rest } = obj`).
/// Such siblings are pulled out solely to exclude their keys from the rest
/// spread — their purpose is the omission, not an individual reference — so an
/// absence of references is by design, not dead code.
///
/// `decl_node` is the declarator (or parameter) that owns the destructuring
/// pattern; oxc records it as the declaration of every name bound within. The
/// pattern is walked downward to locate the binding and check whether its
/// directly enclosing object pattern carries a rest. A binding in an object
/// pattern *without* a rest, the rest binding itself, and a binding nested in an
/// inner pattern that has no rest of its own are all left reportable.
fn is_rest_sibling_destructure(
    decl_node: oxc_semantic::NodeId,
    symbol_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::BindingPattern;

    let root = match semantic.nodes().kind(decl_node) {
        AstKind::VariableDeclarator(decl) => &decl.id,
        AstKind::FormalParameter(param) => &param.pattern,
        _ => return false,
    };

    // Returns true once the binding at `symbol_span` is found as a non-rest
    // property of an object pattern whose `rest` is present.
    fn walk(pattern: &BindingPattern<'_>, target: oxc_span::Span) -> bool {
        match pattern {
            BindingPattern::ObjectPattern(obj) => {
                let has_rest = obj.rest.is_some();
                for prop in &obj.properties {
                    // The value is a `BindingIdentifier` directly (`tags`,
                    // `_nodes: nodes`) or wrapped in a default (`a = 1`); in
                    // either form it is a non-rest leaf of this pattern.
                    if has_rest && binds_identifier_at(&prop.value, target) {
                        return true;
                    }
                    if walk(&prop.value, target) {
                        return true;
                    }
                }
                obj.rest
                    .as_ref()
                    .is_some_and(|rest| walk(&rest.argument, target))
            }
            BindingPattern::ArrayPattern(arr) => {
                arr.elements
                    .iter()
                    .flatten()
                    .any(|el| walk(el, target))
                    || arr
                        .rest
                        .as_ref()
                        .is_some_and(|rest| walk(&rest.argument, target))
            }
            BindingPattern::AssignmentPattern(assign) => walk(&assign.left, target),
            BindingPattern::BindingIdentifier(_) => false,
        }
    }

    // True when `pattern` binds a single identifier whose span is `target`,
    // possibly behind a default value (`a = 1`).
    fn binds_identifier_at(pattern: &BindingPattern<'_>, target: oxc_span::Span) -> bool {
        match pattern {
            BindingPattern::BindingIdentifier(id) => id.span == target,
            BindingPattern::AssignmentPattern(assign) => {
                binds_identifier_at(&assign.left, target)
            }
            _ => false,
        }
    }

    walk(root, symbol_span)
}

/// True when `decl_node` is a class declaration carrying a decorator that
/// registers it as a custom element (`@customElement('tag')`). The decorated
/// class is reached through its HTML tag name rather than a JavaScript reference,
/// so an absence of identifier references is by design, not dead code.
fn is_custom_element_class(
    decl_node: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let AstKind::Class(class) = semantic.nodes().kind(decl_node) else {
        return false;
    };
    class.decorators.iter().any(|decorator| {
        let callee = match &decorator.expression {
            // `@customElement('tag')` — registering form: the decorator invokes
            // a factory that calls `customElements.define(...)`.
            Expression::CallExpression(call) => &call.callee,
            // `@customElement` (no call) — defensive, same registering identifier.
            other => other,
        };
        matches!(callee, Expression::Identifier(id) if is_custom_element_decorator_name(&id.name))
    })
}

/// Collects the factory identifiers named by per-file JSX pragma comments:
/// `@jsx X` (element factory) and `@jsxFrag X` (fragment factory). A file
/// beginning with `/** @jsx jsx */` is the per-file equivalent of tsconfig's
/// `jsxFactory`: every `<element>` compiles to `jsx('element', ...)` and every
/// `<>` to a `Fragment` call, so the named binding is consumed by the JSX
/// transform without ever appearing as an explicit source reference. Both the
/// block (`/** @jsx X */`) and line (`// @jsx X`) comment forms are honored.
fn jsx_pragma_factories(semantic: &oxc_semantic::Semantic, source: &str) -> HashSet<String> {
    let mut factories = HashSet::new();
    for comment in semantic.comments() {
        let Some(text) = source.get(comment.span.start as usize..comment.span.end as usize) else {
            continue;
        };
        // The factory name is the whitespace-separated token following the
        // `@jsx` / `@jsxFrag` tag, e.g. `@jsx jsx` / `@jsxFrag Fragment`.
        let mut tokens = text.split_whitespace().peekable();
        while let Some(token) = tokens.next() {
            if (token == "@jsx" || token == "@jsxFrag")
                && let Some(&factory) = tokens.peek()
                && is_identifier(factory)
            {
                factories.insert(factory.to_string());
            }
        }
    }
    factories
}

/// True when `s` is a plausible JS identifier: it starts with a letter, `_`, or
/// `$` and contains only identifier characters. Guards against treating prose
/// (`@jsx is the pragma`) as a factory name.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// True when `decl_node` is an import binding (named, default, or namespace
/// specifier). Used to scope the JSX-pragma exemption to imported factories — a
/// local variable that happens to share the pragma name is not exempt.
fn is_import_binding(decl_node: oxc_semantic::NodeId, semantic: &oxc_semantic::Semantic) -> bool {
    matches!(
        semantic.nodes().kind(decl_node),
        AstKind::ImportSpecifier(_)
            | AstKind::ImportDefaultSpecifier(_)
            | AstKind::ImportNamespaceSpecifier(_)
    )
}

/// True when the program contains any JSX element or fragment.
fn file_contains_jsx(semantic: &oxc_semantic::Semantic) -> bool {
    semantic
        .nodes()
        .iter()
        .any(|node| matches!(node.kind(), AstKind::JSXElement(_) | AstKind::JSXFragment(_)))
}

/// Collects every enum member referenced in *type position* as
/// `EnumName.Member` — the discriminated-union literal-type pattern
/// (`readonly kind: ReaderStateKind.Header`). oxc resolves the enum name itself
/// but not the trailing member, so these references are invisible to the symbol
/// reference count and the member would otherwise be flagged as unused.
///
/// Each pair is `(enum symbol id, member name)`: `ReaderStateKind.Header`
/// records `(ReaderStateKind, "Header")` and counts only as a use of `Header`,
/// never `Body`. The left side is resolved through its `reference_id` so the
/// match is symbol-accurate, not name-based — an unrelated `Other.Header` in the
/// same file does not exempt the enum's `Header`.
fn collect_type_position_enum_member_uses(
    semantic: &oxc_semantic::Semantic,
) -> HashSet<(SymbolId, String)> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut uses = HashSet::new();

    for node in nodes.iter() {
        let AstKind::TSQualifiedName(qualified) = node.kind() else {
            continue;
        };
        let TSTypeName::IdentifierReference(left) = &qualified.left else {
            continue;
        };
        let Some(ref_id) = left.reference_id.get() else {
            continue;
        };
        let Some(enum_symbol) = scoping.get_reference(ref_id).symbol_id() else {
            continue;
        };
        if !matches!(
            nodes.kind(scoping.symbol_declaration(enum_symbol)),
            AstKind::TSEnumDeclaration(_)
        ) {
            continue;
        }
        uses.insert((enum_symbol, qualified.right.name.to_string()));
    }

    uses
}

/// True when the enum member declared at `decl_node` is referenced in type
/// position as `EnumName.Member` — i.e. `(enclosing enum symbol, member name)`
/// is present in `type_position_uses`. Returns false for any non-enum-member
/// declaration, so a genuinely unreferenced member (value or type) stays
/// reportable.
fn is_enum_member_used_in_type_position(
    decl_node: oxc_semantic::NodeId,
    member_name: &str,
    type_position_uses: &HashSet<(SymbolId, String)>,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    if !matches!(nodes.kind(decl_node), AstKind::TSEnumMember(_)) {
        return false;
    }
    let Some(enum_symbol) = nodes.ancestor_kinds(decl_node).find_map(|kind| match kind {
        AstKind::TSEnumDeclaration(decl) => decl.id.symbol_id.get(),
        _ => None,
    }) else {
        return false;
    };
    type_position_uses.contains(&(enum_symbol, member_name.to_string()))
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
        // Memoized once a JSX-factory pragma import is encountered; scanning
        // every node for JSX is only worth paying for when there's such an
        // import.
        let mut has_jsx: Option<bool> = None;
        // Memoized on the first import binding that looks unused; scanning the
        // comments for `@jsx`/`@jsxFrag` pragmas is only paid for when a file
        // declares an import that would otherwise be flagged.
        let mut jsx_factories: Option<HashSet<String>> = None;
        // Memoized on the first unreferenced enum member; building the
        // type-position use set scans every node, so it is only paid for in
        // files that actually declare an enum member that looks unused.
        let mut enum_member_type_uses: Option<HashSet<(SymbolId, String)>> = None;

        for symbol_id in scoping.symbol_ids() {
            let name = scoping.symbol_name(symbol_id);
            if name.starts_with('_') || name.is_empty() {
                continue;
            }
            if scoping.get_resolved_references(symbol_id).next().is_some() {
                continue;
            }

            let decl_node = scoping.symbol_declaration(symbol_id);
            let symbol_span = scoping.symbol_span(symbol_id);

            if is_named_function_expression_id(decl_node, semantic) {
                continue;
            }

            if is_custom_element_class(decl_node, semantic) {
                continue;
            }

            if is_rest_sibling_destructure(decl_node, symbol_span, semantic) {
                continue;
            }

            // An enum member referenced only as `EnumName.Member` in a type
            // position (discriminated-union literal type) is used; oxc resolves
            // the enum name but not the trailing member, so it is invisible to
            // the reference count above.
            if matches!(nodes.kind(decl_node), AstKind::TSEnumMember(_))
                && is_enum_member_used_in_type_position(
                    decl_node,
                    name,
                    enum_member_type_uses
                        .get_or_insert_with(|| collect_type_position_enum_member_uses(semantic)),
                    semantic,
                )
            {
                continue;
            }

            if is_react_pragma_import(decl_node, semantic)
                && *has_jsx.get_or_insert_with(|| file_contains_jsx(semantic))
            {
                continue;
            }

            // A binding named by a per-file `/** @jsx X */` / `// @jsx X` (or
            // `@jsxFrag X`) pragma is the JSX factory: every JSX element/fragment
            // in the file compiles to a call on it, so it is consumed by the
            // transform without an explicit source reference. Gated on the file
            // containing JSX so a stray pragma in a non-JSX file does not exempt.
            if is_import_binding(decl_node, semantic)
                && jsx_factories
                    .get_or_insert_with(|| jsx_pragma_factories(semantic, ctx.source))
                    .contains(name)
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
            // Parameters of a type-level function signature — a call, construct,
            // method, or index signature, or a function/constructor type — are
            // documentation-only labels: the signature has no body, so the names
            // are never bound and can never be referenced. They are reported only
            // by their enclosing type construct, not as variables.
            let in_type_decl = nodes.ancestor_kinds(decl_node).any(|k| {
                matches!(
                    k,
                    AstKind::TSTypeAliasDeclaration(_)
                        | AstKind::TSInterfaceDeclaration(_)
                        | AstKind::TSModuleDeclaration(_)
                        | AstKind::TSGlobalDeclaration(_)
                        | AstKind::TSFunctionType(_)
                        | AstKind::TSConstructorType(_)
                        | AstKind::TSCallSignatureDeclaration(_)
                        | AstKind::TSConstructSignatureDeclaration(_)
                        | AstKind::TSMethodSignature(_)
                        | AstKind::TSIndexSignature(_)
                        | AstKind::TSMappedType(_)
                ) || matches!(k, AstKind::Function(f) if f.body.is_none())
            });
            if decl_is_type_construct || in_type_decl {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, symbol_span.start as usize);
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
    fn no_fp_on_rest_sibling_destructure() {
        // Bindings destructured alongside a rest element exist solely to exclude
        // their keys from the rest spread (`...jsonValues`); their purpose is the
        // omission, not an individual reference. (Closes #1606)
        let src = r#"
const machineSnapshotToJSON = function toJSON(this: AnyMachineSnapshot) {
    const {
        _nodes: nodes,
        tags,
        machine,
        getMeta,
        toJSON,
        can,
        hasTag,
        matches,
        ...jsonValues
    } = this;
    return { ...jsonValues, tags: Array.from(tags) };
};
export { machineSnapshotToJSON };
"#;
        let diags = run(src);
        for name in ["nodes", "machine", "getMeta", "can", "hasTag", "matches"] {
            assert!(
                !diags.iter().any(|d| d.message.contains(&format!("`{name}`"))),
                "FP on rest-sibling destructured binding `{name}`: {diags:?}"
            );
        }
    }

    #[test]
    fn still_flags_unused_destructure_without_rest_sibling() {
        // Negative-space guard: a destructured binding with no rest element in
        // its pattern is genuinely unused dead code and must still fire.
        let src = "const { a, b } = obj;\nconsole.log(b);\nexport {};";
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`a`")),
            "expected unused destructured `a` (no rest sibling) to be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_custom_element_decorated_class() {
        // A class decorated with `@customElement('tag')` is registered in the
        // browser's custom-element registry as a side effect and reached through
        // its HTML tag name, never a JavaScript reference — so an unreferenced
        // local class is live, not dead code. (Closes #1805)
        let src = r#"
import { customElement } from 'lit/decorators.js';

@customElement('row-virtualizer-dynamic')
class RowVirtualizerDynamic extends LitElement {}
export {};
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("RowVirtualizerDynamic")),
            "FP on @customElement-decorated class: {diags:?}"
        );
    }

    #[test]
    fn still_flags_undecorated_unused_class() {
        // Negative-space guard for #1805 — a class with no registering decorator
        // and no reference is genuinely dead code and must still fire.
        let src = "class UnusedWidget extends LitElement {}\nexport {};";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected `UnusedWidget` to be flagged: {diags:?}");
        assert!(diags[0].message.contains("UnusedWidget"));
    }

    #[test]
    fn still_flags_class_with_unrelated_decorator() {
        // Negative-space guard for #1805 — the exemption is scoped to
        // custom-element-registering decorators. A class decorated with an
        // unrelated decorator (`@sealed`) is not registered as a custom element,
        // so an unused one stays dead code.
        let src = r#"
function sealed(target: unknown) { return target; }

@sealed
class UnusedSealed {}
export {};
"#;
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("UnusedSealed")),
            "expected `UnusedSealed` (unrelated decorator) to be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_call_signature_param_labels() {
        // Parameter names in type-level call signatures (`(foo: number): string`
        // inside a type literal / interface) are documentation-only labels, not
        // variable bindings — they can never be referenced. (Closes #1302)
        let src = r#"
declare type WritableNamespace = {
    (foo: number): string;
};
declare const variation11: {
    (a1: string, a2: number): boolean;
    p1?: string;
    readonly p2: number;
};
export {};
"#;
        let diags = run(src);
        for name in ["foo", "a1", "a2"] {
            assert!(
                !diags.iter().any(|d| d.message.contains(&format!("`{name}`"))),
                "FP on call-signature parameter label `{name}`: {diags:?}"
            );
        }
    }

    #[test]
    fn no_fp_on_method_and_construct_signature_param_labels() {
        // Method signatures (`m(x: number): void`), construct signatures
        // (`new (y: string): T`), and index signatures (`[key: string]: V`) in a
        // type literal / interface carry documentation-only parameter labels.
        // (Closes #1302)
        let src = r#"
interface Shape {
    draw(width: number, height: number): void;
    new (label: string): Shape;
    [index: string]: unknown;
}
export {};
"#;
        let diags = run(src);
        for name in ["width", "height", "label", "index"] {
            assert!(
                !diags.iter().any(|d| d.message.contains(&format!("`{name}`"))),
                "FP on signature parameter label `{name}`: {diags:?}"
            );
        }
    }

    #[test]
    fn still_flags_unused_param_in_function_implementation() {
        // Negative-space guard for #1302 — a genuinely unused parameter in an
        // actual function implementation (with a body) is a real binding and
        // stays reportable.
        let src = "function f(used: number, unusedParam: number) {\n  return used;\n}\nexport { f };";
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`unusedParam`")),
            "expected unused implementation parameter `unusedParam` to be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_enum_member_used_as_literal_type_discriminant() {
        // Enum members used exclusively as `EnumName.Member` literal types in a
        // discriminated union — the standard TS pattern — are referenced only at
        // the type level. oxc resolves the enum name but not the trailing
        // member, so the value-usage reference count misses them. (Closes #2181)
        let src = r#"
enum ReaderStateKind {
    Header = 0,
    Body = 1,
}

interface ReaderStateHeader {
    readonly kind: ReaderStateKind.Header;
    contentLength?: number;
}

interface ReaderStateBody {
    readonly kind: ReaderStateKind.Body;
    readonly contentLength: number;
}

export type ReaderState = ReaderStateHeader | ReaderStateBody;
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`Header`") || d.message.contains("`Body`")),
            "FP on enum members used only as literal type discriminants: {diags:?}"
        );
    }

    #[test]
    fn still_flags_enum_member_never_referenced() {
        // Negative-space guard for #2181 — only the members actually named in a
        // type position are exempted. `Body` is referenced as
        // `ReaderStateKind.Body`; `Unused` is referenced nowhere (value or
        // type), so it stays reportable.
        let src = r#"
enum ReaderStateKind {
    Body = 1,
    Unused = 2,
}

export interface ReaderStateBody {
    readonly kind: ReaderStateKind.Body;
}
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`Body`")),
            "FP on enum member used as literal type discriminant: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`Unused`")),
            "expected enum member `Unused` (referenced nowhere) to be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_jsx_pragma_factory_import() {
        // A per-file `/** @jsx jsx */` pragma names a custom JSX factory: every
        // `<element>` compiles to `jsx('element', ...)`, so the import is
        // consumed by the JSX transform even though it has no explicit call
        // site. oxc's reference count only sees source references. (Closes #2176)
        let src = r#"/** @jsx jsx */
import { jsx } from '../..'

export const input = (
  <editor>
    <block>
      <text />
    </block>
  </editor>
)
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`jsx`")),
            "FP on `@jsx jsx` pragma factory import: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_jsx_pragma_line_comment_form() {
        // The line-comment form `// @jsx jsx` is the equivalent per-file pragma
        // and must be honored too. (Closes #2176)
        let src = "// @jsx jsx\nimport { jsx } from '../..'\n\nexport const input = <editor />\n";
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`jsx`")),
            "FP on `// @jsx jsx` line-comment pragma: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_jsx_frag_pragma_fragment_factory() {
        // `@jsxFrag Fragment` names the fragment factory consumed by every `<>`
        // fragment in the file; it is exempted alongside the element factory.
        // (Closes #2176)
        let src = r#"/** @jsx h */
/** @jsxFrag Fragment */
import { h, Fragment } from 'preact'

export const view = (
  <>
    <div />
  </>
)
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`h`")),
            "FP on `@jsx h` pragma factory import: {diags:?}"
        );
        assert!(
            !diags.iter().any(|d| d.message.contains("`Fragment`")),
            "FP on `@jsxFrag Fragment` pragma fragment factory import: {diags:?}"
        );
    }

    #[test]
    fn still_flags_unused_import_beside_jsx_pragma_factory() {
        // Negative-space guard for #2176 — the pragma exempts only the named
        // factory. A genuinely unused unrelated import in the same file is still
        // dead code.
        let src = r#"/** @jsx jsx */
import { jsx } from '../..'
import { unused } from './other'

export const input = <editor />
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`jsx`")),
            "FP on `@jsx jsx` pragma factory import: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`unused`")),
            "expected unused unrelated import `unused` to be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_jsx_pragma_factory_import_without_jsx() {
        // Negative-space guard for #2176 — a `@jsx jsx` pragma in a file with no
        // JSX consumes nothing, so an unused `jsx` import is genuinely dead code.
        let src = "/** @jsx jsx */\nimport { jsx } from '../..'\nexport {};";
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`jsx`")),
            "expected unused `jsx` import (no JSX in file) to be flagged: {diags:?}"
        );
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
