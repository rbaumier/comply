//! Thin wrapper around oxc_parser + oxc_semantic for rules that need
//! true scope analysis (cross-scope reference tracking, shadowing,
//! unused symbols) instead of the heuristic tree-sitter walks.
//!
//! `oxc_ast` borrows from a bump `Allocator` for the whole AST lifetime,
//! so we expose a closure-based API instead of returning the `Semantic`:
//! the allocator lives on the stack of `with_semantic` and gets dropped
//! when the closure returns.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::SourceType;

/// Pick the right `SourceType` based on file extension. Defaults to `tsx()`
/// for unknown extensions — it's the most permissive (accepts JSX +
/// TypeScript syntax).
pub fn source_type_for_path(path: &Path) -> SourceType {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts") => SourceType::ts(),
        Some("tsx") => SourceType::tsx(),
        Some("mjs") => SourceType::mjs(),
        Some("cjs") => SourceType::cjs(),
        Some("jsx") => SourceType::jsx(),
        _ => SourceType::tsx(),
    }
}

#[cfg(test)]
pub fn with_semantic<F, R>(source: &str, source_type: SourceType, f: F) -> R
where
    F: for<'a> FnOnce(&'a Semantic<'a>) -> R,
{
    let allocator = Allocator::default();
    let parse_ret = Parser::new(&allocator, source, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    f(&semantic)
}

/// Convert an oxc byte offset into 1-based `(line, column)`.
///
/// Shared across all `OxcCheck` rules that emit diagnostics — avoids the
/// copy-pasted per-rule helper that was duplicated in 15+ tree-sitter rules.
pub fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\r' {
            continue;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Parse `source` with oxc_parser using the source type inferred from `path`,
/// build semantic analysis, then hand the `Semantic` to `f`. The allocator
/// and AST are dropped after `f` returns.
///
/// Used by the engine hot path for `Backend::Oxc` dispatch.
pub fn with_oxc_parse<F, R>(source: &str, path: &Path, f: F) -> R
where
    F: for<'a> FnOnce(&'a Semantic<'a>) -> R,
{
    let source_type = source_type_for_path(path);
    let allocator = Allocator::default();
    let parse_ret = Parser::new(&allocator, source, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    f(&semantic)
}

/// TanStack Query / Solid Query / Vue Query factory calls whose options
/// object accepts callbacks with library-dictated signatures (`onError`
/// gets `(error, variables, context, mutation)`, `queryFn` gets a context
/// object, etc.). When the user writes those callbacks they have no say
/// over the arity — flagging them with `max-params` is a guaranteed false
/// positive.
const TANSTACK_QUERY_FACTORIES: &[&str] = &[
    "useMutation",
    "useQuery",
    "useInfiniteQuery",
    "useQueries",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
    "useSuspenseQueries",
    "createMutation",
    "createQuery",
    "createInfiniteQuery",
    // Per-call callback options on the mutation result object: these accept
    // the same fixed-signature callbacks as the factory options object.
    "mutate",
    "mutateAsync",
];

/// Option-keys inside a TanStack Query factory call whose value is a
/// callback with a fixed signature dictated by the library types.
const TANSTACK_QUERY_CALLBACK_KEYS: &[&str] = &[
    "onError",
    "onSuccess",
    "onSettled",
    "onMutate",
    "mutationFn",
    "queryFn",
    "getNextPageParam",
    "getPreviousPageParam",
];

/// True when `node` is a function expression / arrow function being passed
/// as a known third-party callback whose signature is dictated by the
/// outer call's type — e.g. `useMutation({ onError: (a, b, c, d) => ... })`.
///
/// Recognises:
/// 1. `node` is the value of an object property in an object literal.
/// 2. That object literal is one of the arguments of a CallExpression
///    (any position — TanStack Query v4 accepts
///    `useQuery(queryKey, queryFn, options)`).
/// 3. The CallExpression's callee identifier is one of
///    [`TANSTACK_QUERY_FACTORIES`].
/// 4. The property name is one of [`TANSTACK_QUERY_CALLBACK_KEYS`].
///
/// All four must hold. The match is purely name-based — hand-rolled
/// look-alikes are out of scope (the user can rename their helper or open
/// an issue to add it to the allowlist).
#[must_use]
pub fn is_fixed_signature_library_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::{Expression, PropertyKey};

    let nodes = semantic.nodes();
    let node_span = {
        use oxc_span::GetSpan;
        match node.kind() {
            AstKind::Function(f) => f.span(),
            AstKind::ArrowFunctionExpression(a) => a.span(),
            _ => return false,
        }
    };

    // Walk up to the enclosing ObjectProperty.
    let mut current_id = node.id();
    let object_property_key: &str;
    let object_property_node_id;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::ObjectProperty(prop) = parent.kind() {
            // The function must be the property's *value*, not nested
            // somewhere deeper (e.g. a default expression).
            use oxc_span::GetSpan;
            let value_span = prop.value.span();
            if value_span != node_span {
                return false;
            }
            object_property_key = match &prop.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => return false,
            };
            object_property_node_id = parent_id;
            break;
        }
        current_id = parent_id;
    }

    if !TANSTACK_QUERY_CALLBACK_KEYS.contains(&object_property_key) {
        return false;
    }

    // The property's parent must be an ObjectExpression that is the first
    // argument of a CallExpression whose callee identifier is in the
    // factory allowlist.
    let obj_parent_id = nodes.parent_id(object_property_node_id);
    if obj_parent_id == object_property_node_id {
        return false;
    }
    let obj_parent = nodes.get_node(obj_parent_id);
    let AstKind::ObjectExpression(obj_expr) = obj_parent.kind() else {
        return false;
    };

    let call_parent_id = nodes.parent_id(obj_parent_id);
    if call_parent_id == obj_parent_id {
        return false;
    }
    let call_parent = nodes.get_node(call_parent_id);
    let AstKind::CallExpression(call) = call_parent.kind() else {
        return false;
    };

    // Any argument may be this ObjectExpression — TanStack Query v4 supports
    // the overloaded `useQuery(queryKey, queryFn, options)` shape where the
    // options object is the third argument.
    use oxc_span::GetSpan;
    let obj_expr_span = obj_expr.span();
    let matches_any_arg = call.arguments.iter().any(|arg| {
        arg.as_expression()
            .is_some_and(|expr| expr.span() == obj_expr_span)
    });
    if !matches_any_arg {
        return false;
    }

    // Callee identifier in allowlist. Handles both bare calls (`useMutation`)
    // and namespace-import calls (`RQ.useMutation`) — the receiver is not
    // verified to be a namespace import; property name is sufficient.
    let callee_name = match &call.callee {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        _ => return false,
    };
    TANSTACK_QUERY_FACTORIES.contains(&callee_name)
}

/// True when `name` matches a generic type parameter declared on any enclosing
/// function, method, class, interface, or type alias of `node`.
#[must_use]
pub fn name_is_generic_type_param_in_scope(
    name: &str,
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    for ancestor in semantic.nodes().ancestors(node_id) {
        let type_params = match ancestor.kind() {
            AstKind::Function(f) => f.type_parameters.as_deref(),
            AstKind::ArrowFunctionExpression(a) => a.type_parameters.as_deref(),
            AstKind::Class(c) => c.type_parameters.as_deref(),
            AstKind::TSInterfaceDeclaration(i) => i.type_parameters.as_deref(),
            AstKind::TSTypeAliasDeclaration(a) => a.type_parameters.as_deref(),
            AstKind::TSMethodSignature(m) => m.type_parameters.as_deref(),
            AstKind::TSCallSignatureDeclaration(c) => c.type_parameters.as_deref(),
            AstKind::TSConstructSignatureDeclaration(c) => c.type_parameters.as_deref(),
            _ => None,
        };
        if let Some(tp_decl) = type_params {
            for tp in &tp_decl.params {
                if tp.name.name.as_str() == name {
                    return true;
                }
            }
        }
    }
    false
}
