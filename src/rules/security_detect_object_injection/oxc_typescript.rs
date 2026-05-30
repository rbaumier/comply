//! security-detect-object-injection oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSLiteral, TSType, TSTypeName, TSTypeOperatorOperator};
use std::sync::Arc;

const ITER_METHODS: &[&str] = &[
    "map", "forEach", "flatMap", "filter", "find", "reduce", "some", "every",
];

/// Bound on type-alias resolution recursion (`type A = B`, `type B = "x" | A`).
const MAX_ALIAS_DEPTH: usize = 8;

/// Max parent nodes walked from a callback arrow function up to its `CallExpression`:
/// `ArrowFunction` → `Argument` (wrapper) → `Arguments` (list) → `CallExpression` ≤ 3 hops.
const MAX_CALLBACK_ANCESTOR_DEPTH: usize = 3;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(member) = node.kind() else {
            return;
        };
        // Skip when the key is a literal (string / number / template
        // with no interpolations) — that's a static lookup, not an
        // injection vector.
        match &member.expression {
            Expression::StringLiteral(_) | Expression::NumericLiteral(_) => return,
            Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() => return,
            _ => {}
        }
        // Skip array literal access `arr[0]` and similar — the rule
        // targets OBJECT injection, not array indexing. Heuristic:
        // if the object is itself an array literal, skip.
        if matches!(&member.object, Expression::ArrayExpression(_)) {
            return;
        }
        // Skip when the key expression itself carries a `keyof`-bounded type
        // assertion: `obj[key as keyof typeof obj]` or `obj[<keyof T>key]`.
        match &member.expression {
            Expression::TSAsExpression(as_expr)
                if ts_type_has_keyof(&as_expr.type_annotation) =>
            {
                return;
            }
            Expression::TSTypeAssertion(ta) if ts_type_has_keyof(&ta.type_annotation) => {
                return;
            }
            _ => {}
        }
        // Skip when the key's static type contains a `keyof` operator — bounded by TS at the call site.
        if let Expression::Identifier(id_ref) = &member.expression
            && let Some(ref_id) = id_ref.reference_id.get()
            && let Some(sym_id) = semantic.scoping().get_reference(ref_id).symbol_id()
        {
            let decl_id = semantic.scoping().symbol_declaration(sym_id);
            if param_type_has_keyof(decl_id, semantic)
                || is_iterator_callback_over_keyof_array(decl_id, semantic)
                || key_type_is_closed_literal_union(decl_id, semantic)
                || is_for_in_variable(decl_id, semantic)
                || is_for_of_over_keyof_iterable(decl_id, semantic)
            {
                return;
            }
        }
        // Assignment `obj[key] = …` still flags — write is even riskier
        // than read.
        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bracket access with a non-literal key — vulnerable to prototype \
                      pollution / data exfiltration if the key comes from untrusted \
                      input. Validate the key against an allowlist first."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Resolve `decl_id` to its declared TS type annotation (formal parameter or
/// variable declarator, walking up through nested binding patterns) and test
/// it with `pred`. Untyped bindings and function/program boundaries yield
/// `false`. oxc points `symbol_declaration` at the FormalParameter node for
/// parameter bindings; for nested binding patterns we walk up.
fn decl_type_annotation_satisfies(
    decl_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
    pred: impl Fn(&TSType) -> bool,
) -> bool {
    let nodes = semantic.nodes();
    let check = |ann: &Option<oxc_allocator::Box<'_, oxc_ast::ast::TSTypeAnnotation<'_>>>| {
        ann.as_ref().is_some_and(|a| pred(&a.type_annotation))
    };
    match nodes.kind(decl_id) {
        AstKind::FormalParameter(param) => return check(&param.type_annotation),
        AstKind::VariableDeclarator(decl) => return check(&decl.type_annotation),
        _ => {}
    }
    for kind in nodes.ancestor_kinds(decl_id) {
        match kind {
            AstKind::FormalParameter(param) => return check(&param.type_annotation),
            AstKind::VariableDeclarator(decl) => return check(&decl.type_annotation),
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_)
            | AstKind::VariableDeclaration(_) => return false,
            _ => continue,
        }
    }
    false
}

/// True when `decl_id` resolves to a binding whose type annotation contains a
/// `keyof X` operator anywhere in the type tree, including via a type alias.
fn param_type_has_keyof(
    decl_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    decl_type_annotation_satisfies(decl_id, semantic, |ty| {
        ts_type_has_keyof(ty) || type_ref_alias_has_keyof(ty, semantic, MAX_ALIAS_DEPTH)
    })
}

/// True when `ty` is a plain type-reference (`Foo` or `Foo<T>`) whose alias
/// definition contains a `keyof` operator. Handles both non-generic aliases
/// (`type Breakpoint = keyof typeof BREAKPOINTS`) and generic ones
/// (`type FilterKey<T> = keyof T & string`).
fn type_ref_alias_has_keyof(
    ty: &TSType,
    semantic: &oxc_semantic::Semantic<'_>,
    depth: usize,
) -> bool {
    if depth == 0 {
        return false;
    }
    let TSType::TSTypeReference(r) = ty else {
        return false;
    };
    let TSTypeName::IdentifierReference(id) = &r.type_name else {
        return false;
    };
    resolve_alias_has_keyof(id.name.as_str(), semantic, depth - 1)
}

/// Find a `type <name> = …` alias declaration and test whether its definition
/// contains a `keyof` operator.
fn resolve_alias_has_keyof(
    name: &str,
    semantic: &oxc_semantic::Semantic<'_>,
    depth: usize,
) -> bool {
    if depth == 0 {
        return false;
    }
    for node in semantic.nodes().iter() {
        if let AstKind::TSTypeAliasDeclaration(alias) = node.kind()
            && alias.id.name.as_str() == name
        {
            return ts_type_has_keyof(&alias.type_annotation);
        }
    }
    false
}

/// True when `decl_id` resolves to a binding whose type annotation is a closed
/// union of string/number literals — `"a" | "b"` — directly or via a type
/// alias. Such a key can never carry an out-of-set value without a type
/// assertion (flagged separately), so the bracket access is safe by
/// construction.
fn key_type_is_closed_literal_union(
    decl_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    decl_type_annotation_satisfies(decl_id, semantic, |ty| {
        is_closed_literal_key_type(ty, semantic, MAX_ALIAS_DEPTH)
    })
}

/// True when `ty` is a string/number literal, a union of such, or a type-alias
/// reference resolving to one. `depth` bounds alias-chain recursion.
fn is_closed_literal_key_type(
    ty: &TSType,
    semantic: &oxc_semantic::Semantic<'_>,
    depth: usize,
) -> bool {
    if depth == 0 {
        return false;
    }
    match ty {
        TSType::TSLiteralType(lit) => matches!(
            &lit.literal,
            TSLiteral::StringLiteral(_) | TSLiteral::NumericLiteral(_)
        ),
        TSType::TSUnionType(u) => u
            .types
            .iter()
            .all(|t| is_closed_literal_key_type(t, semantic, depth)),
        TSType::TSParenthesizedType(p) => {
            is_closed_literal_key_type(&p.type_annotation, semantic, depth)
        }
        TSType::TSTypeReference(r) => match &r.type_name {
            TSTypeName::IdentifierReference(id) => {
                resolve_alias_is_literal_union(id.name.as_str(), semantic, depth - 1)
            }
            _ => false,
        },
        _ => false,
    }
}

/// Find a `type <name> = …` alias declaration and test whether its definition
/// is a closed literal union.
fn resolve_alias_is_literal_union(
    name: &str,
    semantic: &oxc_semantic::Semantic<'_>,
    depth: usize,
) -> bool {
    if depth == 0 {
        return false;
    }
    for node in semantic.nodes().iter() {
        if let AstKind::TSTypeAliasDeclaration(alias) = node.kind()
            && alias.id.name.as_str() == name
        {
            return is_closed_literal_key_type(&alias.type_annotation, semantic, depth);
        }
    }
    false
}

/// True when the binding is the parameter of an iterator-callback (`ITER_METHODS`)
/// whose receiver array's static type contains a `keyof X` operator.
fn is_iterator_callback_over_keyof_array(
    decl_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    if !matches!(nodes.kind(decl_id), AstKind::FormalParameter(_)) {
        return false;
    }
    // Walk to the enclosing arrow/function.
    let mut func_id = None;
    for (kind, nid) in nodes.ancestor_kinds(decl_id).zip(nodes.ancestor_ids(decl_id)) {
        match kind {
            AstKind::FormalParameters(_) => continue,
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                func_id = Some(nid);
                break;
            }
            _ => break,
        }
    }
    let Some(func_id) = func_id else { return false };
    // Find the enclosing CallExpression (skipping the Argument wrapper).
    let mut cur = nodes.parent_id(func_id);
    if cur == func_id {
        return false;
    }
    for _ in 0..MAX_CALLBACK_ANCESTOR_DEPTH {
        if let AstKind::CallExpression(call) = nodes.kind(cur) {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            if !ITER_METHODS.contains(&member.property.name.as_str()) {
                return false;
            }
            // Receiver is a type-asserted expression: `(keys as Array<keyof T>).forEach`.
            if expression_type_has_keyof(&member.object) {
                return true;
            }
            let Expression::Identifier(recv) = &member.object else {
                return false;
            };
            let Some(ref_id) = recv.reference_id.get() else {
                return false;
            };
            let Some(sym_id) = semantic.scoping().get_reference(ref_id).symbol_id() else {
                return false;
            };
            let recv_decl = semantic.scoping().symbol_declaration(sym_id);
            return param_type_has_keyof(recv_decl, semantic);
        }
        let next = nodes.parent_id(cur);
        if next == cur {
            break;
        }
        cur = next;
    }
    false
}

/// True when `decl_id` is the loop variable of a `for...in` statement.
/// Such keys are string properties of the iterated object and are safe for
/// bracket access on that same object.
fn is_for_in_variable(
    decl_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    if !matches!(nodes.kind(decl_id), AstKind::VariableDeclarator(_)) {
        return false;
    }
    for kind in nodes.ancestor_kinds(decl_id) {
        match kind {
            AstKind::VariableDeclaration(_) => continue,
            AstKind::ForInStatement(_) => return true,
            _ => return false,
        }
    }
    false
}

/// True when `decl_id` is the loop variable of a `for...of` statement whose
/// iterable expression carries a type assertion (`as T` or `<T>`) whose type
/// contains a `keyof` operator — e.g.
/// `for (const key of Object.keys(obj) as Array<keyof typeof obj>)`.
fn is_for_of_over_keyof_iterable(
    decl_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    if !matches!(nodes.kind(decl_id), AstKind::VariableDeclarator(_)) {
        return false;
    }
    for kind in nodes.ancestor_kinds(decl_id) {
        match kind {
            AstKind::VariableDeclaration(_) => continue,
            AstKind::ForOfStatement(stmt) => {
                return expression_type_has_keyof(&stmt.right);
            }
            _ => return false,
        }
    }
    false
}

/// True when `expr` carries a type assertion (`as T` or `<T>`) whose type
/// contains a `keyof` operator. Unwraps parentheses and non-null assertions.
fn expression_type_has_keyof(expr: &Expression) -> bool {
    match expr {
        Expression::TSAsExpression(as_expr) => ts_type_has_keyof(&as_expr.type_annotation),
        Expression::TSTypeAssertion(ta) => ts_type_has_keyof(&ta.type_annotation),
        Expression::TSNonNullExpression(nn) => expression_type_has_keyof(&nn.expression),
        Expression::ParenthesizedExpression(p) => expression_type_has_keyof(&p.expression),
        _ => false,
    }
}

/// Recursively check whether a TS type contains a `keyof X` operator.
/// Covers `keyof T`, `keyof T & string`, `readonly (keyof T)[]`,
/// `Extract<keyof T, string>`, and similar shapes.
fn ts_type_has_keyof(ty: &TSType) -> bool {
    match ty {
        TSType::TSTypeOperatorType(op) => {
            op.operator == TSTypeOperatorOperator::Keyof
                || ts_type_has_keyof(&op.type_annotation)
        }
        TSType::TSIntersectionType(i) => i.types.iter().any(ts_type_has_keyof),
        TSType::TSUnionType(u) => u.types.iter().any(ts_type_has_keyof),
        TSType::TSArrayType(a) => ts_type_has_keyof(&a.element_type),
        TSType::TSParenthesizedType(p) => ts_type_has_keyof(&p.type_annotation),
        TSType::TSTypeReference(r) => r
            .type_arguments
            .as_ref()
            .is_some_and(|args| args.params.iter().any(ts_type_has_keyof)),
        TSType::TSConditionalType(c) => {
            ts_type_has_keyof(&c.check_type)
                || ts_type_has_keyof(&c.extends_type)
                || ts_type_has_keyof(&c.true_type)
                || ts_type_has_keyof(&c.false_type)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_dynamic_bracket_access() {
        let src = r#"function f(obj, key) { return obj[key]; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_static_string_key() {
        let src = r#"function f(obj) { return obj["foo"]; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_array_literal_index() {
        let src = r#"const x = ["a", "b", "c"][i];"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_keyof_typed_parameter() {
        let src = r#"
            function pick<T>(obj: T, key: keyof T) {
                return obj[key];
            }
        "#;
        assert!(run(src).is_empty(), "keyof-typed key should not flag");
    }

    #[test]
    fn allows_keyof_intersection_typed_parameter() {
        let src = r#"
            function pick<T>(obj: T, key: keyof T & string) {
                return obj[key];
            }
        "#;
        assert!(
            run(src).is_empty(),
            "`keyof T & string` typed key should not flag"
        );
    }

    // Regression: `const ks: (keyof Foo)[] = …` — variable with explicit keyof
    // type annotation should not flag when used as bracket-access key.
    #[test]
    fn allows_variable_with_keyof_type_annotation() {
        let src = r#"
            interface Foo { a: number; b: string }
            declare const obj: Foo;
            const ks: (keyof Foo)[] = ["a", "b"];
            ks.map((k) => obj[k]);
        "#;
        assert!(
            run(src).is_empty(),
            "const with explicit keyof annotation should not flag"
        );
    }

    // Regression test for issue #118 — iteration over a typed key
    // tuple (`readonly (keyof TSearch & string)[]`) inside `.map`.
    #[test]
    fn issue_118_typed_key_tuple_map() {
        let src = r#"
            type ListRouteSearch = { foo: string | null; bar: string | null };
            function filtersFromSearch<TSearch extends ListRouteSearch>(
                search: TSearch,
                filterKeys: readonly (keyof TSearch & string)[],
            ): Record<string, string | null> {
                return Object.fromEntries(
                    filterKeys.map((key) => {
                        const rawValue: unknown = search[key];
                        return [key, typeof rawValue === "string" ? rawValue : null] as const;
                    }),
                );
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "expected no diagnostics, got {diags:#?}"
        );
    }

    // Regression for #264: a key typed as a string-literal-union alias
    // (`type SessionRole = "admin" | "read"`) is closed — `obj[role]` can't
    // escape the set without an assertion, so it must not flag.
    #[test]
    fn issue_264_literal_union_alias_key() {
        let src = r#"
            type SessionRole = "admin" | "read";
            const ROLE_LABEL: Record<SessionRole, string> = { admin: "A", read: "L" };
            function roleLabel(role: SessionRole): string {
                return ROLE_LABEL[role];
            }
        "#;
        let diags = run(src);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:#?}");
    }

    #[test]
    fn allows_inline_literal_union_key() {
        let src = r#"
            const m: Record<string, number> = {};
            function f(k: "a" | "b"): number { return m[k]; }
        "#;
        assert!(run(src).is_empty());
    }

    // Negative: a plainly `string`-typed key is still an injection vector.
    #[test]
    fn still_flags_string_typed_key() {
        let src = r#"function f(obj: Record<string, number>, key: string) { return obj[key]; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression #365 — for...in loop variable is a property of the iterated
    // object and must not flag.
    #[test]
    fn issue_365_for_in_variable() {
        let src = r#"
            declare const myObj: Record<string, number>;
            for (const key in myObj) {
                const value = myObj[key];
            }
        "#;
        let diags = run(src);
        assert!(diags.is_empty(), "for...in key should not flag, got {diags:#?}");
    }

    // Regression #365 — for...in with object cast on the access site.
    #[test]
    fn issue_365_for_in_with_cast_object() {
        let src = r#"
            declare const myObj: Record<string, number>;
            for (const key in myObj) {
                const value = (myObj as Record<string, unknown>)[key];
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "for...in key with cast object should not flag, got {diags:#?}"
        );
    }

    // Regression #365 — for...of over Object.keys() cast to Array<keyof typeof obj>.
    #[test]
    fn issue_365_for_of_object_keys_cast_to_keyof_array() {
        let src = r#"
            declare const myObj: { a: number; b: string };
            for (const key of Object.keys(myObj) as Array<keyof typeof myObj>) {
                const value = myObj[key];
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "for...of over Object.keys() as Array<keyof> should not flag, got {diags:#?}"
        );
    }

    // Regression #365 — for...of over Object.keys() cast to (keyof typeof obj)[].
    #[test]
    fn issue_365_for_of_object_keys_cast_to_keyof_tuple() {
        let src = r#"
            declare const myObj: { x: number; y: string };
            for (const key of Object.keys(myObj) as (keyof typeof myObj)[]) {
                const value = myObj[key];
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "for...of over Object.keys() as (keyof)[] should not flag, got {diags:#?}"
        );
    }

    // Regression #365 — bracket key is a `key as keyof typeof obj` assertion.
    #[test]
    fn issue_365_key_as_keyof_assertion() {
        let src = r#"
            declare const myObj: { a: number; b: string };
            function f(key: string): number {
                return myObj[key as keyof typeof myObj];
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "key as keyof T assertion should not flag, got {diags:#?}"
        );
    }

    // Regression #365 — bracket key is a `key as keyof MyType` named-type assertion.
    #[test]
    fn issue_365_key_as_keyof_named_type_assertion() {
        let src = r#"
            interface MyConfig { host: string; port: number }
            declare const config: MyConfig;
            function getField(key: string): unknown {
                return config[key as keyof MyConfig];
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "key as keyof NamedType assertion should not flag, got {diags:#?}"
        );
    }

    // Regression #365 — forEach callback over Object.keys() cast to keyof array.
    #[test]
    fn issue_365_foreach_over_object_keys_cast() {
        let src = r#"
            declare const myObj: { a: number; b: string };
            (Object.keys(myObj) as Array<keyof typeof myObj>).forEach((key) => {
                const value = myObj[key];
            });
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "forEach over Object.keys() as Array<keyof> should not flag, got {diags:#?}"
        );
    }

    // Regression #444 — `type Breakpoint = keyof typeof BREAKPOINTS` is a type
    // alias resolving to a `keyof` type; `bp: Breakpoint` is safe.
    #[test]
    fn issue_444_keyof_typeof_alias_parameter() {
        let src = r#"
            const BREAKPOINTS = { sm: 640, md: 768, lg: 1024 } as const;
            type Breakpoint = keyof typeof BREAKPOINTS;

            function getBreakpointPx(bp: Breakpoint): number {
                return BREAKPOINTS[bp];
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "keyof-typeof alias parameter should not flag, got {diags:#?}"
        );
    }

    // Regression #444 — a generic alias `FilterKey<T> = keyof T & string` still
    // has `keyof` in its body; `key: FilterKey<TSearch>` is a safe access.
    #[test]
    fn issue_444_generic_keyof_alias_parameter() {
        let src = r#"
            type FilterKey<TSearch> = keyof TSearch & string;
            declare const search: { foo: string; bar: number };
            function f(key: FilterKey<typeof search>): unknown {
                return search[key];
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "generic keyof alias parameter should not flag, got {diags:#?}"
        );
    }
}
