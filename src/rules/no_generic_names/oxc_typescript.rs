//! no-generic-names OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const BANNED_WORDS: &[&str] = &[
    "info", "temp", "result", "obj", "item", "thing", "stuff", "val", "retval", "value", "foo",
    "bar", "row", "rows", "lookup",
];

const PARAM_ALLOWED_WORDS: &[&str] = &["value", "item"];

const BANNED_PREFIXES: &[&str] = &["process", "data", "do", "execute", "run", "perform"];

const GLOBAL_IDENTIFIER_ALLOWLIST: &[&str] = &[
    "process",
    "require",
    "module",
    "exports",
    "Buffer",
    "globalThis",
    "console",
    "__dirname",
    "__filename",
];

const DESCRIPTIVE_SUFFIXES: &[&str] = &[
    "_DIR", "_PATH", "_FILE", "_URL", "_URI", "_KEY", "_ID", "_PORT", "_HOST", "_ADDR", "_SIZE",
    "_LEN", "_COUNT", "_MAX", "_MIN", "_TIMEOUT", "_INTERVAL", "_LIMIT", "_TTL", "_ROOT", "_BASE",
];

/// PascalCase `Data*` UI primitive names exempted from the `data` banned-prefix check.
const DATA_PASCAL_CASE_ALLOWED_COMPOUNDS: &[&str] =
    &["DataTable", "DataGrid", "DataView", "DataList"];

const ITERATOR_METHODS: &[&str] = &[
    "map",
    "filter",
    "find",
    "findIndex",
    "forEach",
    "some",
    "every",
    "flatMap",
    "reduce",
    "sort",
];

/// Return the banned prefix matching `name` on a word boundary, or None.
fn matched_banned_prefix(name: &str) -> Option<&'static str> {
    let bytes = name.as_bytes();
    for &prefix in BANNED_PREFIXES {
        let plen = prefix.len();
        if bytes.len() < plen {
            continue;
        }
        if !bytes[..plen].eq_ignore_ascii_case(prefix.as_bytes()) {
            continue;
        }
        let on_boundary = if bytes.len() == plen {
            true
        } else if bytes[..plen].iter().all(|b| b.is_ascii_uppercase()) {
            if bytes[plen] != b'_' {
                continue;
            }
            let suffix = &name[plen..];
            if DESCRIPTIVE_SUFFIXES
                .iter()
                .any(|s| suffix.eq_ignore_ascii_case(s))
            {
                continue;
            }
            true
        } else {
            bytes[plen].is_ascii_uppercase() || bytes[plen] == b'_'
        };
        if on_boundary {
            // `runWith*` is the idiomatic AsyncLocalStorage wrapper pattern —
            // the `run` comes from `AsyncLocalStorage.run()`, not a generic verb.
            if prefix == "run" && name[plen..].starts_with("With") {
                continue;
            }
            return Some(prefix);
        }
    }
    None
}

/// True when the node is inside a destructuring pattern (object pattern).
fn is_destructuring<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for kind in nodes.ancestor_kinds(node.id()) {
        if matches!(kind, AstKind::ObjectPattern(_)) {
            return true;
        }
        // Stop at statement boundaries
        if matches!(
            kind,
            AstKind::VariableDeclaration(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::Program(_)
        ) {
            break;
        }
    }
    false
}

/// True when the identifier is a function/arrow parameter (inside a FormalParameter).
fn is_function_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for kind in semantic.nodes().ancestor_kinds(node.id()) {
        if matches!(kind, AstKind::FormalParameter(_)) {
            return true;
        }
        if matches!(
            kind,
            AstKind::VariableDeclaration(_)
                | AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::Program(_)
        ) {
            break;
        }
    }
    false
}

/// True when the identifier sits inside an `import { … }` / `import x from …`
/// / `import * as x from …` declaration. The author has no rename freedom
/// for a third-party export (e.g. `import { Result } from "better-result"`).
fn is_in_import_declaration<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for kind in semantic.nodes().ancestor_kinds(node.id()) {
        if matches!(kind, AstKind::ImportDeclaration(_)) {
            return true;
        }
    }
    false
}

/// Look at the surrounding FormalParameter / VariableDeclarator's type
/// annotation and decide whether it carries enough domain information to
/// justify a generic name. We accept three shapes:
///
/// - A type reference named `Result` (e.g. `result: Result<User, Err>`).
/// - A type reference named `Promise<Result<…>>` (the awaited form).
/// - An array type (`readonly TRow[]`, `User[]`, `Array<User>`) — for
///   helpers like `firstOrError<TRow>(rows: readonly TRow[], …)`.
/// - A type assertion on the initializer (`const rows = expr as T[]`) —
///   covers generic DB helpers where the cast carries the type information.
fn type_annotation_is_descriptive<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for kind in nodes.ancestor_kinds(node.id()) {
        match kind {
            AstKind::FormalParameter(p) => {
                let Some(ann) = p.type_annotation.as_ref() else {
                    return false;
                };
                return ts_type_is_descriptive(&ann.type_annotation);
            }
            AstKind::VariableDeclarator(d) => {
                if let Some(ann) = d.type_annotation.as_ref() {
                    return ts_type_is_descriptive(&ann.type_annotation);
                }
                // No explicit annotation: accept a type assertion on the
                // initializer (`const rows = expr as InferSelectModel<TRef>[]`).
                return init_has_descriptive_type_assertion(d.init.as_ref());
            }
            // Stop walking when we leave the binding's surroundings.
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => continue,
        }
    }
    false
}

/// True when `init` is a `TSAsExpression` (or parenthesized wrapper) whose
/// asserted type passes `ts_type_is_descriptive`. Handles:
///   `expr as T[]`  →  TSAsExpression { type_annotation: TSArrayType }
fn init_has_descriptive_type_assertion(init: Option<&Expression>) -> bool {
    let Some(expr) = init else { return false };
    match expr {
        Expression::TSAsExpression(as_expr) => ts_type_is_descriptive(&as_expr.type_annotation),
        Expression::ParenthesizedExpression(paren) => {
            init_has_descriptive_type_assertion(Some(&paren.expression))
        }
        _ => false,
    }
}

fn ts_type_is_descriptive(ty: &TSType) -> bool {
    match ty {
        // `Result<...>`, `Promise<Result<...>>`, etc.
        TSType::TSTypeReference(type_ref) => {
            let name = match &type_ref.type_name {
                TSTypeName::IdentifierReference(id) => Some(id.name.as_str()),
                TSTypeName::QualifiedName(q) => Some(q.right.name.as_str()),
                _ => None,
            };
            if matches!(name, Some("Result")) {
                return true;
            }
            // TypeScript generic type parameter convention: single capital
            // letter (T, R, E, K, V) or T followed by uppercase (TData,
            // TResult, TVariables). These carry type information via the
            // caller's generic instantiation.
            if let Some(n) = name {
                let b = n.as_bytes();
                if (b.len() == 1 && b[0].is_ascii_uppercase())
                    || (b.len() >= 2 && b[0] == b'T' && b[1].is_ascii_uppercase())
                {
                    return true;
                }
            }
            // `Promise<Result<...>>` / `Readonly<Foo[]>` / `Array<T>` —
            // recurse into the type argument.
            if matches!(name, Some("Promise") | Some("Readonly") | Some("Array") | Some("ReadonlyArray")) {
                if let Some(params) = &type_ref.type_arguments
                    && let Some(first) = params.params.first()
                {
                    return ts_type_is_descriptive(first);
                }
            }
            false
        }
        // `T[]` / `readonly T[]`.
        TSType::TSArrayType(_) => true,
        // `readonly T[]` shows up as TSTypeOperator (op = Readonly) wrapping
        // a TSArrayType in oxc's AST.
        TSType::TSTypeOperatorType(op) => ts_type_is_descriptive(&op.type_annotation),
        // `Result | null`, `Result | undefined`.
        TSType::TSUnionType(u) => u.types.iter().any(ts_type_is_descriptive),
        _ => false,
    }
}

/// True if the identifier is a parameter of an iterator callback (.map, .filter, etc.).
fn is_iterator_callback_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up: FormalParameter -> FormalParameters -> Function/Arrow -> Argument -> CallExpression
    let mut func_id = None;
    for (kind, nid) in nodes.ancestor_kinds(node.id()).zip(nodes.ancestor_ids(node.id())) {
        match kind {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                func_id = Some(nid);
                break;
            }
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => continue,
            _ => break,
        }
    }
    let Some(func_id) = func_id else {
        return false;
    };
    // The function must be a direct argument of a call expression whose callee
    // is a member expression with a property in ITERATOR_METHODS.
    let parent_id = nodes.parent_id(func_id);
    if parent_id == func_id {
        return false;
    }
    // Walk up through Argument wrapper to CallExpression
    let mut cur = parent_id;
    for _ in 0..3 {
        let kind = nodes.kind(cur);
        if let AstKind::CallExpression(call) = kind {
            if let Expression::StaticMemberExpression(ref member) = call.callee {
                let method = member.property.name.as_str();
                return ITERATOR_METHODS.contains(&method);
            }
            let callee_text =
                &source[call.callee.span().start as usize..call.callee.span().end as usize];
            if let Some(method) = callee_text.rsplit('.').next() {
                return ITERATOR_METHODS.contains(&method);
            }
            return false;
        }
        let next = nodes.parent_id(cur);
        if next == cur {
            break;
        }
        cur = next;
    }
    false
}

/// True when the identifier is a function parameter inside an arrow function
/// or function expression that is used as an object property value.
/// Covers TanStack Query callbacks like `useMutation({ onSuccess: (data) => {} })`
/// where the library's own API prescribes the parameter name.
fn is_in_callback_property<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut func_node_id = None;
    for (kind, nid) in nodes.ancestor_kinds(node.id()).zip(nodes.ancestor_ids(node.id())) {
        match kind {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                func_node_id = Some(nid);
                break;
            }
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => continue,
            _ => break,
        }
    }
    let Some(fid) = func_node_id else { return false };
    let parent_id = nodes.parent_id(fid);
    if parent_id == fid {
        return false;
    }
    matches!(nodes.kind(parent_id), AstKind::ObjectProperty(_))
}

/// True when the identifier is a property key in an object literal.
fn is_object_literal_key<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent_kind = nodes.kind(parent_id);
    if let AstKind::ObjectProperty(prop) = parent_kind {
        // Check if we're the key, not the value
        let key_span = match &prop.key {
            PropertyKey::StaticIdentifier(id) => Some(id.span),
            _ => None,
        };
        if let Some(ks) = key_span {
            let node_span = node.kind().span();
            return ks.start == node_span.start && ks.end == node_span.end;
        }
    }
    false
}

/// True when the identifier is a method call property (`obj.execute()`).
fn is_method_call_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent_kind = nodes.kind(parent_id);
    if let AstKind::StaticMemberExpression(member) = parent_kind {
        // We must be the property
        let node_span = node.kind().span();
        if member.property.span.start == node_span.start {
            // And the member must be called
            let gp_id = nodes.parent_id(parent_id);
            if gp_id != parent_id {
                return matches!(nodes.kind(gp_id), AstKind::CallExpression(_));
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IdentifierReference, AstType::BindingIdentifier]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir || ctx.file.path_segments.in_storybook {
            return;
        }

        let (name, span) = match node.kind() {
            AstKind::BindingIdentifier(id) => (id.name.as_str(), id.span),
            AstKind::IdentifierReference(id) => (id.name.as_str(), id.span),
            _ => return,
        };

        // Identifiers introduced by `import { Foo } from "..."` cannot
        // be renamed by the author — third-party exports are out of their
        // control. Same for default / namespace imports.
        if is_in_import_declaration(node, semantic) {
            return;
        }

        // Check banned words — only at declaration sites (BindingIdentifier)
        if let AstKind::BindingIdentifier(_) = node.kind()
            && !is_destructuring(node, semantic)
                && !is_iterator_callback_param(node, semantic, ctx.source)
            {
                let lower = name.to_ascii_lowercase();
                if BANNED_WORDS.contains(&lower.as_str()) {
                    if PARAM_ALLOWED_WORDS.contains(&lower.as_str())
                        && is_function_param(node, semantic)
                    {
                        return;
                    }
                    // A descriptive type annotation (`result: Result<…>`,
                    // `rows: readonly TRow[]`) carries the domain info the
                    // identifier name would otherwise need to.
                    if type_annotation_is_descriptive(node, semantic) {
                        return;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Identifier '{name}' carries no meaning — rename to describe \
                             what the value IS (`parsedOrder`, `userProfile`, \
                             `paymentReceipt`)."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }

        // Check banned prefixes — only at declaration sites (BindingIdentifier).
        // Checking references would re-flag every usage of a destructured name
        // like `data` from `const { data } = useQuery(...)`.
        if !matches!(node.kind(), AstKind::BindingIdentifier(_)) {
            return;
        }
        if is_destructuring(node, semantic) {
            return;
        }
        if is_object_literal_key(node, semantic) {
            return;
        }
        if is_method_call_name(node, semantic) {
            return;
        }
        if GLOBAL_IDENTIFIER_ALLOWLIST.contains(&name) {
            return;
        }

        if let Some(prefix) = matched_banned_prefix(name) {
            if prefix == "data" && DATA_PASCAL_CASE_ALLOWED_COMPOUNDS.iter().any(|allowed| match name.strip_prefix(allowed) {
                Some("") => true,
                Some(rest) => rest.as_bytes().first().is_some_and(|b| b.is_ascii_uppercase()),
                None => false,
            }) {
                return;
            }
            // A descriptive type annotation (e.g. `data: TData`) carries
            // the domain information the identifier name would otherwise need.
            if type_annotation_is_descriptive(node, semantic) {
                return;
            }
            // The function is a property value in an object literal
            // (e.g. TanStack Query's `onSuccess: (data) => {}`). The
            // library's own API prescribes `data` as the parameter name.
            if is_in_callback_property(node, semantic) {
                return;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Identifier '{name}' uses banned prefix '{prefix}' — use \
                     intent over implementation. Try: what does this actually \
                     accomplish? (`processOrder` → `fulfillOrder`, `doPayment` → \
                     `chargeCustomer`)."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_bare_result_local_const() {
        let src = r#"function f() { const result = 1; return result; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_result_imported_from_better_result() {
        // Regression for rbaumier/comply#39 case 1 — third-party imports.
        let src = r#"import { Result } from "better-result"; const x: Result<number, Error> = anything;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_result_default_import() {
        // Regression for #214 — default imports.
        let src = r#"import Result from "better-result";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_result_namespace_import() {
        // Regression for #214 — namespace imports.
        let src = r#"import * as Result from "better-result";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_renamed_named_import_to_result() {
        // Regression for #214 — renamed local binding.
        let src = r#"import { Foo as Result } from "lib";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_user_declared_result_const() {
        // Negative: user-declared, not imported — must still flag.
        let src = r#"const Result = somethingElse;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_user_declared_result_function() {
        // Negative: user-declared function — must still flag.
        let src = r#"function Result() { return 1; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_result_when_typed_as_Result() {
        // Regression for rbaumier/comply#39 case 2 — canonical Result name.
        let src = r#"
            async function unwrapOrThrow<T, E>(p: Promise<Result<T, E>>): Promise<T> {
                const result: Result<T, E> = await p;
                if (result.isErr()) { throw result.error; }
                return result.value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_rows_parameter_typed_as_array() {
        // Regression for rbaumier/comply#39 case 3 — generic-helper rows.
        let src = r#"
            function firstOrError<TRow>(rows: readonly TRow[], message: string): TRow {
                return rows[0];
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_untyped_rows_param() {
        let src = r#"function f(rows) { return rows[0]; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_data_table_design_system_compound() {
        // Regression for rbaumier/comply#121 — `DataTable` and its derived
        // type names are the industry-standard naming across shadcn,
        // TanStack Table, Material UI, and Radix.
        let src = r#"
            export function DataTable() { return null; }
            export type DataTableSort = { field: string };
            export type DataTableState = { sort: DataTableSort };
            export type DataGridColumn = { key: string };
            export type DataViewProps = { rows: number };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_data_prefix_on_non_canonical_compounds() {
        // `DataSource`, `DataObject`, `DataValue` carry no more meaning than
        // `data` alone — the `Data` allowlist is intentionally narrow to the
        // canonical UI primitives.
        let src = r#"const dataSource = 1; const DataObject = {}; const DataValue = 2;"#;
        assert_eq!(run(src).len(), 3);
    }

    #[test]
    fn no_fp_data_param_typed_with_generic_type_param() {
        // Regression for #337 — TanStack Query type aliases annotate the
        // `data` parameter with a generic type parameter like `TData`.
        let src = r#"
            type InvalidateOption<TData, TVariables> =
              | ((data: TData, variables: TVariables) => string[])
              | null;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_data_param_in_tanstack_mutation_callback() {
        // Regression for #337 — TanStack Query's onSuccess callback has
        // `data` as its first parameter per the library's own type signature.
        let src = r#"
            useMutation({
                onSuccess: (data, variables) => {
                    console.log(data);
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_data_param_untyped_in_function_declaration() {
        // `data` without a type annotation in a named function declaration
        // is still vague — no library convention prescribes it there.
        let src = r#"function myFunc(data) { return data; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_rows_with_type_assertion_in_generic_db_helper() {
        // Regression for #389 — `rows` cast to a generic array type in a
        // generic database helper where no domain-specific name is possible.
        let src = r#"
            export async function replaceTeamJunction<
                TJunction extends PgTable,
                TRef extends PgTable,
            >(tx: DatabaseTransaction): Promise<void> {
                return Result.gen(async function* () {
                    const rows = (yield* Result.await(
                        tryDatabaseQuery(() => tx.select().from(junctionTable)),
                    )) as InferSelectModel<TRef>[];
                    return Result.ok(rows);
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_rows_without_type_assertion() {
        // No `as T[]` — must still flag.
        let src = r#"
            async function genericHelper<TRef extends PgTable>(tx: any): Promise<void> {
                const rows = await tx.select().from(table);
                return rows;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_run_with_context_async_local_storage_wrapper() {
        // Regression for #520 — `runWith*` is the idiomatic AsyncLocalStorage
        // wrapper pattern; `run` comes from the Node.js API, not a generic verb.
        let src = r#"
            export function runWithRequestContext<T>(context: RequestContext, callback: () => T): T {
                return requestContextStorage.run(context, callback);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_run_with_transaction_async_local_storage_wrapper() {
        // Same pattern for transaction context.
        let src = r#"
            export function runWithTransaction<T>(tx: Transaction, fn: () => T): T {
                return transactionStorage.run(tx, fn);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_run_task_generic_verb() {
        // `runTask` uses `run` as a generic verb — must still flag.
        let src = r#"function runTask() {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_run_migration_generic_verb() {
        // `runMigration` uses `run` as a generic verb — must still flag.
        let src = r#"function runMigration() {}"#;
        assert_eq!(run(src).len(), 1);
    }
}
