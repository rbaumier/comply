//! ts-no-unused-vars OxcCheck backend — accurate unused-symbol detection via
//! oxc_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_custom_element_decorator_name};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, FunctionType, TSTypeName};
use oxc_semantic::SymbolId;
use rustc_hash::FxHashSet;
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

/// True when `decl_node` is a named `h`/`Fragment` import from a Preact-like
/// package (`preact`, `preact/compat`, `preact/jsx-runtime`). Under Preact's
/// classic JSX transform (`jsxFactory: "h"`, `jsxFragmentFactory: "Fragment"`),
/// each JSX element compiles to an `h(...)` call and each fragment to a
/// `Fragment` reference, so the binding is consumed by the JSX in the file even
/// though it never appears as an explicit source reference. oxc's semantic
/// analysis only sees source references, so it reports this factory import as
/// unused.
fn is_preact_jsx_factory_import(
    decl_node: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::ImportSpecifier(spec) = nodes.kind(decl_node) else {
        return false;
    };
    if !matches!(spec.local.name.as_str(), "h" | "Fragment") {
        return false;
    }
    nodes.ancestor_kinds(decl_node).any(|k| {
        matches!(
            k,
            AstKind::ImportDeclaration(import)
                if matches!(
                    import.source.value.as_str(),
                    "preact" | "preact/compat" | "preact/jsx-runtime"
                )
        )
    })
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

/// True when the binding declared at `decl_node` belongs to an ambient
/// `declare const` / `declare let` / `declare var` statement. Ambient variable
/// declarations are type-only: they are erased at compile time and have no
/// runtime presence, so they can never be referenced at runtime and "unused" is
/// meaningless for them. They exist purely to assert a type — e.g. a tsd-style
/// type probe `declare const x: Foo<Bar>;` whose type annotation *is* the
/// assertion.
///
/// `decl_node` is the `VariableDeclarator`; the `declare` modifier lives on its
/// enclosing `VariableDeclaration`. Only the immediate declaration is checked so
/// a non-ambient `const x = 5;` stays reportable.
fn is_ambient_declare_variable(
    decl_node: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    if !matches!(nodes.kind(decl_node), AstKind::VariableDeclarator(_)) {
        return false;
    }
    nodes
        .ancestor_kinds(decl_node)
        .find_map(|kind| match kind {
            AstKind::VariableDeclaration(decl) => Some(decl.declare),
            _ => None,
        })
        .unwrap_or(false)
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
fn jsx_pragma_factories(semantic: &oxc_semantic::Semantic, source: &str) -> FxHashSet<String> {
    let mut factories = FxHashSet::default();
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

/// Resolves an `IdentifierReference` (the left side of `EnumName.Member`) to the
/// `SymbolId` of an enum it names, or `None` if the reference is unresolved or
/// its symbol is not an enum declaration. Used to attribute a member access to
/// the enum's own symbol so the match is symbol-accurate, not name-based.
fn resolve_enum_symbol(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> Option<SymbolId> {
    let scoping = semantic.scoping();
    let ref_id = ident.reference_id.get()?;
    let enum_symbol = scoping.get_reference(ref_id).symbol_id()?;
    matches!(
        semantic.nodes().kind(scoping.symbol_declaration(enum_symbol)),
        AstKind::TSEnumDeclaration(_)
    )
    .then_some(enum_symbol)
}

/// Collects every enum member referenced as `EnumName.Member`, in either type or
/// value position — type-level discriminants (`readonly kind:
/// ReaderStateKind.Header`), value member access (`ByteMarker.Array`), and
/// computed value access (`ByteMarker["Array"]`). oxc resolves the enum name
/// itself but not the trailing member, so these references are invisible to the
/// symbol reference count and the member would otherwise be flagged as unused.
///
/// Each pair is `(enum symbol id, member name)`: `ReaderStateKind.Header`
/// records `(ReaderStateKind, "Header")` and counts only as a use of `Header`,
/// never `Body`. The enum side is resolved through its `reference_id` so the
/// match is symbol-accurate, not name-based — an unrelated `Other.Header` in the
/// same file does not exempt the enum's `Header`, and a member whose name
/// shadows a built-in global (`String`, `Number`, …) is still attributed to its
/// own enum.
fn collect_enum_member_uses(semantic: &oxc_semantic::Semantic) -> FxHashSet<(SymbolId, String)> {
    let nodes = semantic.nodes();
    let mut uses = FxHashSet::default();

    for node in nodes.iter() {
        match node.kind() {
            // Type position: `EnumName.Member` as a literal type.
            AstKind::TSQualifiedName(qualified) => {
                let TSTypeName::IdentifierReference(left) = &qualified.left else {
                    continue;
                };
                if let Some(enum_symbol) = resolve_enum_symbol(left, semantic) {
                    uses.insert((enum_symbol, qualified.right.name.to_string()));
                }
            }
            // Value position: `EnumName.Member`.
            AstKind::StaticMemberExpression(member) => {
                let Expression::Identifier(obj) = &member.object else {
                    continue;
                };
                if let Some(enum_symbol) = resolve_enum_symbol(obj, semantic) {
                    uses.insert((enum_symbol, member.property.name.to_string()));
                }
            }
            // Value position: `EnumName["Member"]`.
            AstKind::ComputedMemberExpression(member) => {
                let Expression::Identifier(obj) = &member.object else {
                    continue;
                };
                let Expression::StringLiteral(key) = &member.expression else {
                    continue;
                };
                if let Some(enum_symbol) = resolve_enum_symbol(obj, semantic) {
                    uses.insert((enum_symbol, key.value.to_string()));
                }
            }
            _ => {}
        }
    }

    uses
}

/// True when the enum member declared at `decl_node` is referenced as
/// `EnumName.Member` (type or value position) — i.e. `(enclosing enum symbol,
/// member name)` is present in `member_uses`. Returns false for any non-enum-member
/// declaration, so a genuinely unreferenced member stays reportable.
fn is_enum_member_used(
    decl_node: oxc_semantic::NodeId,
    member_name: &str,
    member_uses: &FxHashSet<(SymbolId, String)>,
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
    member_uses.contains(&(enum_symbol, member_name.to_string()))
}

/// True when `decl_node` is a parameter of a class method whose enclosing class
/// has an `implements` clause. Such a method's signature is dictated by the
/// interface contract: a parameter it ignores cannot be dropped or renamed
/// without breaking the contract, so an absence of references is by design, not
/// dead code (e.g. stub/noop interface-method implementations).
///
/// Scoped to *parameters* (`FormalParameter` is the decl node) of the method
/// *directly* enclosing them — the first function-like ancestor must be a class
/// method. A parameter of a closure nested inside such a method, an unused
/// local, or a method in a class with no `implements` clause is left reportable.
fn is_param_of_implements_class_method(
    decl_node: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    if !matches!(nodes.kind(decl_node), AstKind::FormalParameter(_)) {
        return false;
    }
    // The first function-like ancestor must be the method owning this
    // parameter; a closure nested inside the method would shadow it and the
    // parameter would no longer be contract-mandated.
    let mut ancestors = nodes.ancestors(decl_node).peekable();
    let in_class_method = loop {
        match ancestors.next().map(|node| node.kind()) {
            Some(AstKind::Function(_)) => {
                break matches!(ancestors.peek().map(|node| node.kind()), Some(AstKind::MethodDefinition(_)));
            }
            Some(AstKind::ArrowFunctionExpression(_)) => break false,
            Some(_) => continue,
            None => break false,
        }
    };
    if !in_class_method {
        return false;
    }
    nodes.ancestor_kinds(decl_node).find_map(|kind| match kind {
        AstKind::Class(class) => Some(!class.implements.is_empty()),
        _ => None,
    }) == Some(true)
}

/// True when any binding identifier reachable in `pattern` — a simple
/// identifier, or any leaf of a destructuring / rest pattern — has a resolved
/// reference. Used to decide whether a parameter *position* is consumed,
/// regardless of whether it destructures.
fn pattern_has_referenced_binding(
    pattern: &oxc_ast::ast::BindingPattern<'_>,
    scoping: &oxc_semantic::Scoping,
) -> bool {
    use oxc_ast::ast::BindingPattern;
    match pattern {
        BindingPattern::BindingIdentifier(id) => id.symbol_id.get().is_some_and(|sym| {
            scoping.get_resolved_references(sym).next().is_some()
        }),
        BindingPattern::ObjectPattern(obj) => {
            obj.properties
                .iter()
                .any(|prop| pattern_has_referenced_binding(&prop.value, scoping))
                || obj
                    .rest
                    .as_ref()
                    .is_some_and(|rest| pattern_has_referenced_binding(&rest.argument, scoping))
        }
        BindingPattern::ArrayPattern(arr) => {
            arr.elements
                .iter()
                .flatten()
                .any(|el| pattern_has_referenced_binding(el, scoping))
                || arr
                    .rest
                    .as_ref()
                    .is_some_and(|rest| pattern_has_referenced_binding(&rest.argument, scoping))
        }
        BindingPattern::AssignmentPattern(assign) => {
            pattern_has_referenced_binding(&assign.left, scoping)
        }
    }
}

/// True when the simple-identifier parameter declared at `decl_node` precedes a
/// later parameter, in the same parameter list, whose binding is referenced.
/// Such a leading unused parameter is positionally mandated: dropping or
/// renaming it would shift every parameter after it, so an absence of references
/// is by design, not dead code. This is ESLint's `args: "after-used"` default —
/// a leading unused argument is reported only when no later argument in the same
/// list is used.
///
/// Scoped to simple-identifier parameters (`function f(h, conf, key)`), which is
/// the reported class and the clear-cut positional case: a leading destructured
/// binding can be dropped individually without shifting later parameters, so its
/// per-binding behavior is left unchanged. A trailing unused parameter (none
/// used after it) stays reportable, as does an all-unused list.
fn is_unused_param_before_used(
    decl_node: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::BindingPattern;

    let nodes = semantic.nodes();
    let AstKind::FormalParameter(param) = nodes.kind(decl_node) else {
        return false;
    };
    if !matches!(param.pattern, BindingPattern::BindingIdentifier(_)) {
        return false;
    }
    let AstKind::FormalParameters(params) = nodes.parent_kind(decl_node) else {
        return false;
    };
    let Some(index) = params
        .items
        .iter()
        .position(|item| item.node_id.get() == decl_node)
    else {
        return false;
    };

    let scoping = semantic.scoping();
    // A used binding at any later positional slot — a following parameter or the
    // trailing rest element — makes this leading parameter positionally required.
    params.items[index + 1..]
        .iter()
        .any(|item| pattern_has_referenced_binding(&item.pattern, scoping))
        || params
            .rest
            .as_ref()
            .is_some_and(|rest| pattern_has_referenced_binding(&rest.rest.argument, scoping))
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
        // Syntax-highlighter test fixtures (e.g. bat's
        // `tests/syntax-tests/source/<Lang>/example.ts`) and other mock/fixture
        // trees deliberately declare unused symbols to exercise the highlighter;
        // they are sample source, not functional code with consumers, so unused
        // declarations are by design, not dead code. (Closes #1315)
        if crate::rules::path_utils::is_mock_or_fixture_dir_path(ctx.path) {
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
        let mut jsx_factories: Option<FxHashSet<String>> = None;
        // Memoized on the first unreferenced enum member; building the
        // member-use set scans every node, so it is only paid for in files that
        // actually declare an enum member that looks unused.
        let mut enum_member_uses: Option<FxHashSet<(SymbolId, String)>> = None;

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

            // An ambient `declare const`/`declare let`/`declare var` binding is
            // type-only and erased at compile time, so it can never have a
            // runtime reference — "unused" is meaningless for it (e.g. tsd-style
            // type probes `declare const x: Foo<Bar>;`).
            if is_ambient_declare_variable(decl_node, semantic) {
                continue;
            }

            if is_custom_element_class(decl_node, semantic) {
                continue;
            }

            if is_rest_sibling_destructure(decl_node, symbol_span, semantic) {
                continue;
            }

            // A parameter mandated by an interface contract — a method of a
            // class with an `implements` clause — cannot be dropped or renamed
            // without breaking the contract, so an unreferenced one (stub/noop
            // implementation) is by design, not dead code.
            if is_param_of_implements_class_method(decl_node, semantic) {
                continue;
            }

            // An unused parameter followed by a later used one is positionally
            // mandated — dropping or renaming it would shift every parameter
            // after it — so it is not removable dead code (ESLint's
            // `args: "after-used"` default).
            if is_unused_param_before_used(decl_node, semantic) {
                continue;
            }

            // An enum member referenced as `EnumName.Member` (a type-level
            // discriminant, a value member access, or a computed value access)
            // is used; oxc resolves the enum name but not the trailing member,
            // so it is invisible to the reference count above.
            if matches!(nodes.kind(decl_node), AstKind::TSEnumMember(_))
                && is_enum_member_used(
                    decl_node,
                    name,
                    enum_member_uses.get_or_insert_with(|| collect_enum_member_uses(semantic)),
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

            if is_preact_jsx_factory_import(decl_node, semantic)
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
    fn no_fp_on_preact_h_and_fragment_import_with_jsx() {
        // Preact's classic JSX transform (`jsxFactory: "h"`,
        // `jsxFragmentFactory: "Fragment"`): each JSX element compiles to an
        // `h(...)` call and each fragment to a `Fragment` reference, so the named
        // `h`/`Fragment` imports are used by the JSX even though neither appears
        // as an explicit reference. (Closes #2085)
        let src = r#"
import { h, Fragment } from 'preact';

export default function Component({ children }) {
    return (
        <Fragment>
            <div>{children}</div>
        </Fragment>
    );
}
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`h`")),
            "FP on `h` named import from preact in a JSX file: {diags:?}"
        );
        assert!(
            !diags.iter().any(|d| d.message.contains("`Fragment`")),
            "FP on `Fragment` named import from preact in a JSX file: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_preact_compat_and_jsx_runtime_h_import_with_jsx() {
        // The exemption covers Preact-like subpaths used as the factory source.
        for source in ["preact/compat", "preact/jsx-runtime"] {
            let src = format!(
                "import {{ h }} from '{source}';\nexport const El = () => <div />;\n"
            );
            let diags = run(&src);
            assert!(
                !diags.iter().any(|d| d.message.contains("`h`")),
                "FP on `h` import from {source} in a JSX file: {diags:?}"
            );
        }
    }

    #[test]
    fn still_flags_preact_h_import_without_jsx() {
        // No JSX in the file → the classic transform consumes nothing, so an
        // unused `h` import from preact is genuinely dead code.
        let src = "import { h } from 'preact';\nexport {};";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected `h` to be flagged: {diags:?}");
        assert!(diags[0].message.contains("`h`"));
    }

    #[test]
    fn still_flags_unused_non_factory_preact_import_with_jsx() {
        // The carve-out is scoped to the `h`/`Fragment` factory bindings. An
        // unrelated unused named import from preact is still dead code, even when
        // the file contains JSX.
        let src = r#"
import { h, foo } from 'preact';

export const El = () => <div />;
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`h`")),
            "FP on `h` factory import: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`foo`")),
            "expected unused non-factory preact import to be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_unused_h_import_from_non_preact_source_with_jsx() {
        // The carve-out is scoped to Preact-like sources. An unused `h` from an
        // unrelated package is still dead code, even when the file contains JSX.
        let src = r#"
import { h } from './local-helpers';

export const El = () => <div />;
"#;
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`h`")),
            "expected unused `h` from a non-preact source to be flagged: {diags:?}"
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
    fn no_fp_on_syntax_test_fixture() {
        // Syntax-highlighter test fixtures (bat's
        // `tests/syntax-tests/source/<Lang>/example.ts`) deliberately declare
        // unused symbols to exercise the highlighter; they are sample source
        // with no consumers, so unused declarations are by design. (Closes #1315)
        let src = "let letNumber = 10;\nconst constNumber = 20;\n";
        let path = "tests/syntax-tests/source/TypeScript/example.ts";
        assert!(
            run_at(src, path).is_empty(),
            "FP on unused vars in a syntax-tests fixture file"
        );
    }

    #[test]
    fn still_flags_unused_var_in_ordinary_source() {
        // Negative-space guard for #1315 — a genuinely unused variable in an
        // ordinary source file (outside any fixture directory) must still fire.
        let src = "let letNumber = 10;\nexport {};";
        let diags = run_at(src, "src/foo.ts");
        assert_eq!(diags.len(), 1, "expected `letNumber` to be flagged: {diags:?}");
        assert!(diags[0].message.contains("`letNumber`"));
    }

    #[test]
    fn still_flags_unused_var_in_syntax_tests_substring_dir() {
        // Negative-space guard for #1315 — `syntax-tests` is matched as a path
        // segment, not a substring. A `syntax-tests-data/` directory is a
        // different, ordinary directory and its unused vars stay reportable.
        let src = "let letNumber = 10;\nexport {};";
        let diags = run_at(src, "syntax-tests-data/foo.ts");
        assert_eq!(diags.len(), 1, "expected `letNumber` to be flagged: {diags:?}");
        assert!(diags[0].message.contains("`letNumber`"));
    }

    #[test]
    fn no_fp_on_unused_param_in_implements_class_method() {
        // A class implementing an interface must keep the method parameters the
        // contract mandates, even when the body ignores some of them
        // (stub/noop methods). The signature cannot drop or rename a param
        // without breaking the interface, so an unreferenced one there is by
        // design, not dead code. (Closes #1318)
        let src = r#"
class R implements ITreeRenderer<Element, void, HTMLElement> {
    disposeTemplate(templateData: HTMLElement): void {}
    getTemplateId(element: Element): string {
        return 'default';
    }
    renderElement(element: ITreeNode<Element, void>, index: number, templateData: HTMLElement): void {
        templateData.textContent = 'x';
    }
}
export {};
"#;
        let diags = run(src);
        for name in ["templateData", "element", "index"] {
            assert!(
                !diags.iter().any(|d| d.message.contains(&format!("`{name}`"))),
                "FP on interface-mandated parameter `{name}` in an implements-class method: {diags:?}"
            );
        }
    }

    #[test]
    fn still_flags_unused_param_in_class_without_implements() {
        // Negative-space guard for #1318 — a class with no `implements` clause is
        // not bound by an interface contract, so an unused method parameter is a
        // real binding and stays reportable.
        let src = r#"
class Plain {
    greet(unusedParam: string): string {
        return 'hi';
    }
}
export { Plain };
"#;
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`unusedParam`")),
            "expected unused param in a non-implements class to be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_unused_local_inside_implements_class_method() {
        // Negative-space guard for #1318 — the exemption is scoped strictly to
        // parameters. An unused *local* variable inside an implements-class
        // method is still dead code and must fire.
        let src = r#"
class R implements ITreeRenderer<Element, void, HTMLElement> {
    getTemplateId(element: Element): string {
        const unusedLocal = element;
        return 'default';
    }
}
export {};
"#;
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`unusedLocal`")),
            "expected unused local in an implements-class method to be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_unused_param_in_standalone_function() {
        // Negative-space guard for #1318 — a standalone function (no enclosing
        // implements class) keeps flagging unused params.
        let src = "function f(unusedParam: number): number {\n  return 1;\n}\nexport { f };";
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`unusedParam`")),
            "expected unused param in a standalone function to be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_ambient_declare_type_probe() {
        // tsd-style type probes: `declare const x: T` exists purely to assert
        // that `T` is a valid type expression. The binding is ambient (erased at
        // compile time) and never referenced at runtime — the type-check IS the
        // use. (Closes #3322)
        let src = r#"
type ExtractStrict<T, U extends T> = Extract<T, U>;
type ShirtSize = 'small' | 'medium' | 'large';
// @ts-expect-error
declare const allInvalidShirtSizes: ExtractStrict<ShirtSize, 'skyscraper-large' | 'atom-small'>;
declare const never: never;
declare let probe: string;
declare var ambientVar: number;
export {};
"#;
        let diags = run(src);
        for name in ["allInvalidShirtSizes", "never", "probe", "ambientVar"] {
            assert!(
                !diags.iter().any(|d| d.message.contains(&format!("`{name}`"))),
                "FP on ambient declare type-probe `{name}`: {diags:?}"
            );
        }
    }

    #[test]
    fn still_flags_non_ambient_unused_const() {
        // Negative-space guard for #3322 — a non-ambient `const x = 5;` has a
        // real runtime binding and stays reportable. The exemption is scoped to
        // the `declare` modifier, not to all type-annotated consts.
        let src = "const unusedReal = 5;\nconst unusedTyped: number = 7;\nexport {};";
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`unusedReal`")),
            "expected unused non-ambient `unusedReal` to be flagged: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`unusedTyped`")),
            "expected unused non-ambient `unusedTyped` to be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_enum_member_used_as_value_with_global_shadowing_name() {
        // Enum members accessed as runtime values via `EnumName.Member` are
        // used, even when the member names shadow built-in globals (`Array`,
        // `Number`, `String`, …). The member access is attributed to the enum's
        // own symbol, not the global, so none of these are dead code. (Closes
        // #5275)
        let src = r#"
enum ByteMarker {
    Array = 0x0,
    BigInt = 0x1,
    Number = 0x7,
    Object = 0x8,
    String = 0xA,
    Symbol = 0xB,
    Undefined = 0xD,
}

function op(_marker: ByteMarker): void {}

op(ByteMarker.Array);
op(ByteMarker.BigInt);
op(ByteMarker.Number);
op(ByteMarker.Object);
op(ByteMarker.String);
op(ByteMarker.Symbol);
op(ByteMarker.Undefined);
export {};
"#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "FP on enum members accessed by value via `ByteMarker.Member`: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_enum_member_used_via_computed_value_access() {
        // `EnumName["Member"]` is the computed-access form of the same value
        // usage and must be recognised too. (Closes #5275)
        let src = r#"
enum Kind {
    String = "String",
    Number = "Number",
}
const a = Kind["String"];
const b = Kind["Number"];
console.log(a, b);
export {};
"#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "FP on enum members accessed via computed `Kind[\"Member\"]`: {diags:?}"
        );
    }

    #[test]
    fn still_flags_enum_member_never_referenced_as_value_or_type() {
        // Negative-space guard for #5275 — only members actually accessed via
        // `EnumName.Member` are exempted. `Array` is accessed; `Unused` is
        // referenced nowhere, so it stays reportable, and the shadowing name of
        // `Array` does not blanket-exempt the enum.
        let src = r#"
enum Marker {
    Array = 0,
    Unused = 1,
}
const x = Marker.Array;
console.log(x);
export {};
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`Array`")),
            "FP on enum member `Array` accessed by value: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`Unused`")),
            "expected unused enum member `Unused` to be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_member_access_on_non_enum_with_global_name() {
        // Negative-space guard for #5275 — a `Foo.Array` access on a non-enum
        // (here a plain object const) must not exempt an unrelated unused enum
        // member of the same name. Attribution is by the enum's symbol.
        let src = r#"
const Foo = { Array: 1 };
enum Marker {
    Array = 0,
}
console.log(Foo.Array);
export {};
"#;
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`Array`")),
            "expected unused enum member `Marker.Array` to be flagged despite `Foo.Array`: {diags:?}"
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

    #[test]
    fn no_fp_on_leading_unused_param_before_used() {
        // A leading unused parameter followed by used ones is positionally
        // mandated: `h` cannot be dropped or renamed without shifting `conf`
        // and `key`, which the body uses. ESLint's `args: "after-used"`
        // default. (Closes #7796)
        let src = "function f(h, conf, key) {\n  return conf[key];\n}\nexport { f };";
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`h`")),
            "FP on leading unused param `h` before used `conf`/`key`: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_leading_unused_param_in_method_shorthand() {
        // The method-shorthand form from the repro: `h` is the leading
        // positional parameter and `conf`/`key` after it are used, so `h` is
        // positionally required. (Closes #7796)
        let src = r#"
const componentChild = {
    default(h, conf, key) {
        return conf[key];
    },
};
export { componentChild };
"#;
        let diags = run(src);
        assert!(
            !diags.iter().any(|d| d.message.contains("`h`")),
            "FP on leading unused param `h` in a method shorthand: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_middle_unused_param_before_used_trailer() {
        // `b` is unused but `c` after it is used, so `b` is positionally
        // required; `a`/`c` are used and never flagged. (Closes #7796)
        let src = "const f = (a, b, c) => a + c;\nexport { f };";
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "expected no diagnostics (b positionally required, a/c used): {diags:?}"
        );
    }

    #[test]
    fn still_flags_all_unused_params() {
        // Negative-space guard for #7796 — when NO later parameter is used,
        // every parameter stays reportable (after-used reports the whole list).
        let src = "function f(a, b) {}\nexport { f };";
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`a`")),
            "expected all-unused leading param `a` to still be flagged: {diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`b`")),
            "expected all-unused param `b` to still be flagged: {diags:?}"
        );
    }

    #[test]
    fn still_flags_last_unused_param_after_used() {
        // Negative-space guard for #7796 — a trailing unused parameter has no
        // used parameter after it, so after-used still flags it; `a` is used.
        let src = "const f = (a, b) => a;\nexport { f };";
        let diags = run(src);
        assert!(
            diags.iter().any(|d| d.message.contains("`b`")),
            "expected trailing unused param `b` to still be flagged: {diags:?}"
        );
        assert!(
            !diags.iter().any(|d| d.message.contains("`a`")),
            "used param `a` must not be flagged: {diags:?}"
        );
    }

    #[test]
    fn no_fp_on_leading_underscore_param_before_used() {
        // The leading-`_` exemption is unchanged: `_` is a conventional ignored
        // positional placeholder and `value` after it is used. (Closes #7796)
        let src = "const f = (_, value) => value;\nexport { f };";
        let diags = run(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }

}
