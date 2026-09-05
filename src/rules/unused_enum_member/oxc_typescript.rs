//! unused-enum-member OXC backend — flag TypeScript enum members declared
//! in the current file but never referenced anywhere within that file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BinaryOperator, Expression, IdentifierReference, TSLiteral, TSType, TSTypeName,
    TSTypeQueryExprName,
};
use oxc_semantic::{NodeId, Semantic, SymbolId};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub struct Check;

/// Identifiers that introduce a type-level assertion (Vitest / expect-type /
/// tsd): `expectTypeOf<…>()`, `assertType<…>(value)`. A file using them exercises
/// the type-space of its declarations through the type checker rather than at
/// runtime, so an enum declared as the full set of valid values is "used" by the
/// assertions even when individual members are never referenced as runtime
/// values.
const TYPE_ASSERTION_ROOTS: &[&str] = &["expectTypeOf", "assertType"];

/// True when this file is a TypeScript type-test: either by path (tsd/dtslint
/// `.test-d.ts` / type-test dirs, via [`crate::rules::path_utils::is_type_test_file`])
/// or by carrying a type-level assertion call (`expectTypeOf` / `assertType`).
/// In such files enums are type fixtures whose members deliberately span the
/// type under test, so unreferenced members are intentional — not dead code.
/// Ordinary runtime `.test.ts`/`.spec.ts` files without type assertions are not
/// exempt, so genuinely dead enum members in unit tests are still flagged.
fn is_type_test_context(ctx: &CheckCtx) -> bool {
    ctx.file.is_type_test_file()
        || TYPE_ASSERTION_ROOTS
            .iter()
            .any(|root| crate::oxc_helpers::source_contains(ctx.source, root))
}

/// True when `ident` is a value-position reference to the *whole* enum object —
/// the enum identifier read as a value (`Object.values(Food)`, spreading,
/// argument passing, assignment) rather than navigated into via `Food.Member`.
/// Such a reference exposes every member at runtime, so all members are reachable.
///
/// Two conditions must hold:
///  - the reference is a value read/write (`ReferenceFlags::is_value`), excluding
///    type-position uses (`type X = Food`, `: Food`) which oxc also surfaces as
///    `IdentifierReference` nodes but flags as `Type`;
///  - it is not the *object* of a member access (`Food.Member` / `Food[k]`), which
///    reads a single member and is already tracked individually above.
fn is_whole_enum_value_reference(
    ident: &IdentifierReference,
    ref_node_id: NodeId,
    semantic: &Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    if !semantic.scoping().get_reference(ref_id).flags().is_value() {
        return false;
    }
    let nodes = semantic.nodes();
    let ref_span = ident.span;
    !matches!(
        nodes.kind(nodes.parent_id(ref_node_id)),
        AstKind::StaticMemberExpression(member) if member.object.span() == ref_span
    ) && !matches!(
        nodes.kind(nodes.parent_id(ref_node_id)),
        AstKind::ComputedMemberExpression(member) if member.object.span() == ref_span
    )
}

/// A non-exported enum declared in the current file. TypeScript merges
/// same-named enum declarations into a single symbol, so one entry can carry the
/// members of several declarations.
struct TrackedEnum {
    /// Name as written, for the diagnostic message.
    name: String,
    /// `(member name, 1-based declaration line)`, in declaration order.
    members: Vec<(String, u32)>,
}

/// Every non-exported enum of the file, addressed by the symbol its name binds
/// so that a reference is matched by binding rather than by spelling.
struct TrackedEnums {
    /// One entry per enum symbol, in declaration order.
    decls: Vec<TrackedEnum>,
    /// Enum symbol -> its index in `decls`.
    by_symbol: FxHashMap<SymbolId, usize>,
    /// The declaration nodes, whose subtrees are not usage sites.
    decl_nodes: FxHashSet<NodeId>,
}

impl TrackedEnums {
    /// The index of the tracked enum `ident` binds to. `None` when it binds
    /// something else — a shadowing local, an import, another enum of the same
    /// name — or when oxc left the reference unresolved, in which case it names
    /// no tracked enum and so exempts nothing.
    fn index_of(&self, ident: &IdentifierReference, semantic: &Semantic) -> Option<usize> {
        let reference_id = ident.reference_id.get()?;
        let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
        self.by_symbol.get(&symbol_id).copied()
    }
}

/// Record every member of `decls[index]` as used. Called for constructs that
/// reach the whole enum at once — they expose every member without naming any.
fn mark_all_members_used(index: usize, decl: &TrackedEnum, used: &mut FxHashSet<(usize, String)>) {
    for (member_name, _) in &decl.members {
        used.insert((index, member_name.clone()));
    }
}

/// The member names an indexed access selects out of the `typeof Enum`
/// projection rooted at `query_id`. `None` when the projection is not the object
/// of an indexed access, or when the index can reach any member (`keyof typeof
/// Enum`, a generic parameter, a computed key) — the caller then treats the
/// whole enum as referenced.
fn literal_index_of_projection<'a>(
    query_id: NodeId,
    semantic: &Semantic<'a>,
) -> Option<Vec<&'a str>> {
    let nodes = semantic.nodes();
    // `(typeof Enum)[…]` wraps the query in a parenthesised type.
    let mut child_id = query_id;
    let mut parent_id = nodes.parent_id(child_id);
    while matches!(nodes.kind(parent_id), AstKind::TSParenthesizedType(_)) {
        child_id = parent_id;
        parent_id = nodes.parent_id(child_id);
    }
    let AstKind::TSIndexedAccessType(indexed) = nodes.kind(parent_id) else {
        return None;
    };
    // The projection must be what is indexed, not the index itself.
    if indexed.object_type.span() != nodes.kind(child_id).span() {
        return None;
    }
    string_literal_type_names(&indexed.index_type)
}

/// The string-literal names `ty` spells out: one for a string-literal type, one
/// per branch for a union of them. `None` for any other type, which names no
/// fixed set of members.
fn string_literal_type_names<'a>(ty: &TSType<'a>) -> Option<Vec<&'a str>> {
    match ty {
        TSType::TSParenthesizedType(inner) => string_literal_type_names(&inner.type_annotation),
        TSType::TSLiteralType(literal) => match &literal.literal {
            TSLiteral::StringLiteral(s) => Some(vec![s.value.as_str()]),
            _ => None,
        },
        TSType::TSUnionType(union) => {
            let mut names = Vec::with_capacity(union.types.len());
            for branch in &union.types {
                names.extend(string_literal_type_names(branch)?);
            }
            Some(names)
        }
        _ => None,
    }
}

/// Collect the non-exported enums declared in the file. An exported enum is
/// reachable from other files, so its members are never dead on this evidence.
fn collect_enums(semantic: &Semantic, ctx: &CheckCtx) -> TrackedEnums {
    let nodes = semantic.nodes();
    let mut decls: Vec<TrackedEnum> = Vec::new();
    let mut by_symbol: FxHashMap<SymbolId, usize> = FxHashMap::default();
    let mut decl_nodes: FxHashSet<NodeId> = FxHashSet::default();

    for node in nodes.iter() {
        let AstKind::TSEnumDeclaration(decl) = node.kind() else {
            continue;
        };

        let parent_id = nodes.parent_id(node.id());
        if parent_id != node.id()
            && matches!(nodes.kind(parent_id), AstKind::ExportNamedDeclaration(_))
        {
            continue;
        }
        // Also check if the source text starts with "export ".
        let decl_text = &ctx.source[decl.span.start as usize..decl.span.end as usize];
        if decl_text.starts_with("export ") {
            continue;
        }
        let Some(symbol_id) = decl.id.symbol_id.get() else {
            continue;
        };

        let mut members = Vec::new();
        for member in &decl.body.members {
            let member_name =
                &ctx.source[member.id.span().start as usize..member.id.span().end as usize];
            if member_name.is_empty() {
                continue;
            }
            let (line, _) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
            members.push((member_name.to_string(), line as u32));
        }
        if members.is_empty() {
            continue;
        }

        let index = *by_symbol.entry(symbol_id).or_insert_with(|| {
            decls.push(TrackedEnum {
                name: decl.id.name.as_str().to_string(),
                members: Vec::new(),
            });
            decls.len() - 1
        });
        decls[index].members.extend(members);
        decl_nodes.insert(node.id());
    }

    TrackedEnums {
        decls,
        by_symbol,
        decl_nodes,
    }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["enum"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if is_type_test_context(ctx) {
            return Vec::new();
        }

        // Pass 1: collect enum declarations (non-exported only).
        let tracked = collect_enums(semantic, ctx);
        if tracked.decls.is_empty() {
            return Vec::new();
        }

        // Set of (enum index, member_name) that are referenced.
        let mut used: FxHashSet<(usize, String)> = FxHashSet::default();

        // Pass 2: collect usages (EnumName.MemberName patterns).
        for node in semantic.nodes().iter() {
            // Skip nodes inside enum declarations.
            let mut ancestor_id = node.id();
            let nodes = semantic.nodes();
            let mut skip = false;
            loop {
                if tracked.decl_nodes.contains(&ancestor_id) {
                    skip = true;
                    break;
                }
                let parent_id = nodes.parent_id(ancestor_id);
                if parent_id == ancestor_id {
                    break;
                }
                ancestor_id = parent_id;
            }
            if skip {
                continue;
            }

            match node.kind() {
                AstKind::StaticMemberExpression(member) => {
                    if let Expression::Identifier(obj) = &member.object
                        && let Some(index) = tracked.index_of(obj, semantic)
                    {
                        used.insert((index, member.property.name.as_str().to_string()));
                    }
                }
                // A string-literal key names one member; any other key
                // expression can select any of them, so all are reachable.
                AstKind::ComputedMemberExpression(member) => {
                    if let Expression::Identifier(obj) = &member.object
                        && let Some(index) = tracked.index_of(obj, semantic)
                    {
                        if let Expression::StringLiteral(s) = &member.expression {
                            used.insert((index, s.value.as_str().to_string()));
                        } else {
                            mark_all_members_used(index, &tracked.decls[index], &mut used);
                        }
                    }
                }
                // `expr in EnumName` reads every member value off the compiled
                // enum object at runtime, so all members are reachable.
                AstKind::BinaryExpression(bin) => {
                    if bin.operator == BinaryOperator::In
                        && let Expression::Identifier(rhs) = &bin.right
                        && let Some(index) = tracked.index_of(rhs, semantic)
                    {
                        mark_all_members_used(index, &tracked.decls[index], &mut used);
                    }
                }
                // A value-position reference to the bare enum identifier consumes
                // the whole enum object at runtime — `Object.values(Food)`,
                // `Object.keys(Food)`, `Object.entries(Food)`, spreading, passing
                // it as an argument, etc. all iterate every member dynamically, so
                // all members are reachable.
                AstKind::IdentifierReference(id) => {
                    if let Some(index) = tracked.index_of(id, semantic)
                        && is_whole_enum_value_reference(id, node.id(), semantic)
                    {
                        mark_all_members_used(index, &tracked.decls[index], &mut used);
                    }
                }
                // A qualified name spells out one member, in type space
                // (`x: Food.Pizza`) or through a query (`typeof Food.Pizza`);
                // the walker funnels both spellings into this node. Deeper
                // qualification (`NS.Food.Pizza`) has a qualified name on the
                // left, which names no tracked enum.
                AstKind::TSQualifiedName(qualified) => {
                    if let TSTypeName::IdentifierReference(obj) = &qualified.left
                        && let Some(index) = tracked.index_of(obj, semantic)
                    {
                        used.insert((index, qualified.right.name.as_str().to_string()));
                    }
                }
                // `typeof EnumName` projects the enum object into type space.
                // Most uses of that projection reach every member — `keyof typeof
                // E`, `(typeof E)[keyof typeof E]`, `Record<keyof typeof E, T>`,
                // `x: typeof E` — so the whole enum counts as referenced. Indexing
                // the projection with string-literal types names exactly the
                // members those literals spell out. A query naming no local
                // binding (`typeof import("…")`, `typeof this.x`) exempts nothing.
                AstKind::TSTypeQuery(query) => {
                    if let TSTypeQueryExprName::IdentifierReference(id) = &query.expr_name
                        && let Some(index) = tracked.index_of(id, semantic)
                    {
                        match literal_index_of_projection(node.id(), semantic) {
                            Some(names) => {
                                used.extend(names.into_iter().map(|name| (index, name.to_string())))
                            }
                            None => {
                                mark_all_members_used(index, &tracked.decls[index], &mut used);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Diff: flag unused members.
        let mut diagnostics = Vec::new();
        for (index, decl) in tracked.decls.iter().enumerate() {
            let enum_name = &decl.name;
            for (member_name, line) in &decl.members {
                if !used.contains(&(index, member_name.clone())) {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: *line as usize,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "enum member `{enum_name}.{member_name}` is never referenced in this file."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, source, path)
    }

    #[test]
    fn flags_unused_member() {
        let source = r#"
enum Color {
    Red,
    Green,
    Blue,
}
const x = Color.Red;
const y = Color.Green;
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Blue"));
    }

    #[test]
    fn in_operator_marks_all_members_used() {
        let source = r#"
enum clickableInputTypes {
    'button' = 'button',
    'color' = 'color',
    'file' = 'file',
    'image' = 'image',
    'reset' = 'reset',
    'submit' = 'submit',
    'checkbox' = 'checkbox',
    'radio' = 'radio',
}
function isClickableInput(element: HTMLInputElement) {
    return element.type in clickableInputTypes;
}
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn in_operator_unrelated_enum_still_flags_unused() {
        let source = r#"
enum Looked {
    A,
    B,
}
enum Other {
    X,
    Y,
}
const k = "A" in Looked;
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.message.contains("Other")));
    }

    // Regression for #4986 — a `.test.ts` file driving its enum through
    // `expectTypeOf` type assertions exercises the full type-space; unreferenced
    // members are intentional fixtures, not dead code.
    #[test]
    fn type_test_file_with_expect_type_of_is_not_flagged() {
        let source = r#"
enum DessertMissingValue {
    COOKIE = 'cookie',
    CAKE = 'cake',
    MUFFIN = 'muffin',
    ANOTHER = 'another',
}
const ctxMissingValue = DessertMissingValue.ANOTHER;
expectTypeOf(t('dessert', { context: ctxMissingValue })).toMatchTypeOf<string>();
"#;
        assert!(run_at(source, "test/typescript/custom-types/t.test.ts").is_empty());
    }

    // `assertType` (tsd / @vitest/expect-type) is also a type-assertion root.
    #[test]
    fn assert_type_call_is_not_flagged() {
        let source = r#"
enum Color {
    Red,
    Green,
    Blue,
}
assertType<Color>(Color.Red);
"#;
        assert!(run_at(source, "src/widget.test.ts").is_empty());
    }

    // A tsd/dtslint type-test file (path-based signal) is exempt even without a
    // type-assertion call in the snippet.
    #[test]
    fn type_test_path_is_not_flagged() {
        let source = r#"
enum Color {
    Red,
    Green,
    Blue,
}
const x: Color = Color.Red;
"#;
        assert!(run_at(source, "src/schema.test-d.ts").is_empty());
    }

    // Regression for #6114 — `Object.values(Food)` iterates every member of the
    // enum at runtime, so none of the members are dead even though they are never
    // accessed individually as `Food.Member`.
    #[test]
    fn object_values_marks_all_members_used() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
    Fries = "fries",
}
const foodSchema = { enum: Object.values(Food) } as const;
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn object_keys_and_entries_mark_all_members_used() {
        let keys = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
const k = Object.keys(Food);
"#;
        assert!(run(keys).is_empty());

        let entries = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
const e = Object.entries(Food);
"#;
        assert!(run(entries).is_empty());
    }

    // A bare value-position reference (passing the enum object as an argument /
    // assigning it) likewise consumes all members.
    #[test]
    fn bare_value_reference_marks_all_members_used() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
function register(e: object) {}
register(Food);
"#;
        assert!(run(source).is_empty());
    }

    // A whole-enum value reference to one enum does not exempt an unrelated enum
    // that still has a genuinely dead member.
    #[test]
    fn whole_enum_reference_does_not_exempt_unrelated_enum() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
enum Color {
    Red,
    Green,
    Blue,
}
const all = Object.values(Food);
const r = Color.Red;
const g = Color.Green;
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Blue"));
    }

    // Type-position references to the enum (`type X = Food`, `: Food`) are NOT
    // value uses: oxc surfaces them as `IdentifierReference` nodes but flags them
    // `Type`, so they must not exempt the enum's members from the dead-member
    // check. Both members here are genuinely unreferenced as values.
    #[test]
    fn type_position_reference_does_not_exempt() {
        let alias = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
type X = Food;
"#;
        assert_eq!(run(alias).len(), 2);

        let annotation = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
function f(a: Food) {}
"#;
        assert_eq!(run(annotation).len(), 2);
    }

    // Regression for #6873 — `keyof typeof E` is the canonical way to extract the
    // union of an enum's member names. The union *is* the member names, so every
    // member is referenced even though none is spelled out as `E.Member`.
    #[test]
    fn keyof_typeof_marks_all_members_used() {
        let source = r#"
const enum PureCssLang {
    css = 'css',
}
const enum PreprocessLang {
    less = 'less',
    sass = 'sass',
    scss = 'scss',
    styl = 'styl',
    stylus = 'stylus',
}
const enum PostCssDialectLang {
    sss = 'sugarss',
}
type CssLang =
    | keyof typeof PureCssLang
    | keyof typeof PreprocessLang
    | keyof typeof PostCssDialectLang
export function isCSSRequest(lang: string): lang is CssLang {
    return lang.length > 0
}
"#;
        assert!(run(source).is_empty());
    }

    // `typeof E` reaches every member whatever the enclosing type container is,
    // so the exemption is keyed on the query itself, not on `keyof`.
    #[test]
    fn typeof_enum_marks_all_members_used_in_any_container() {
        let indexed = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
type FoodValue = (typeof Food)[keyof typeof Food];
export const f: FoodValue = "pizza";
"#;
        assert!(run(indexed).is_empty());

        let mapped = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
type Counts = Record<keyof typeof Food, number>;
export const c: Counts = { Pizza: 0, Taco: 0 };
"#;
        assert!(run(mapped).is_empty());

        let annotation = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
declare function register(all: typeof Food): void;
"#;
        assert!(run(annotation).is_empty());
    }

    // A qualified query (`typeof E.Member`) reads a single member, so the other
    // members stay dead.
    #[test]
    fn qualified_typeof_marks_only_the_named_member_used() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
export type Pizza = typeof Food.Pizza;
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Taco"));
    }

    // A query that cannot name a local binding (`typeof import("…")`,
    // `typeof this.x`) exempts nothing.
    #[test]
    fn non_identifier_type_query_does_not_exempt() {
        let import_type = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
export type M = typeof import("./mod");
"#;
        assert_eq!(run(import_type).len(), 2);

        let this_type = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
export class Menu {
    y = 1;
    z: typeof this.y = 2;
}
"#;
        assert_eq!(run(this_type).len(), 2);
    }

    // `typeof` on one enum does not exempt an unrelated enum's dead member.
    #[test]
    fn typeof_enum_does_not_exempt_unrelated_enum() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
enum Color {
    Red,
    Green,
    Blue,
}
type FoodName = keyof typeof Food;
export const n: FoodName = "Pizza";
const r = Color.Red;
const g = Color.Green;
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Blue"));
    }

    // Regression for #8379 — a computed key that is not a string literal can
    // select any member, so no member of that enum is provably dead.
    #[test]
    fn dynamic_computed_key_marks_all_members_used() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
export function pick(name: 'Pizza' | 'Taco') {
    return Food[name];
}
"#;
        assert!(run(source).is_empty());
    }

    // A string-literal computed key stays precise: it names exactly one member.
    #[test]
    fn string_literal_computed_key_marks_only_that_member() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
const t = Food["Taco"];
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Pizza"));
    }

    // A dynamic access on one enum does not exempt an unrelated enum's dead
    // member.
    #[test]
    fn dynamic_computed_key_does_not_exempt_unrelated_enum() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
enum Color {
    Red,
    Green,
    Blue,
}
export function pick(k: string) {
    return Food[k as 'Pizza' | 'Taco'];
}
const r = Color.Red;
const g = Color.Green;
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Blue"));
    }

    // Regression for #8380 — a member named in type position (`x: Food.Pizza`)
    // spells the member out just like `typeof Food.Pizza` does.
    #[test]
    fn type_position_qualified_name_marks_only_the_named_member_used() {
        let annotation = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
declare const x: Food.Pizza;
"#;
        let diags = run(annotation);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Taco"));

        // Nested inside type arguments.
        let type_argument = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
export type NotPizza = Exclude<Food, Food.Pizza>;
"#;
        let diags = run(type_argument);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Taco"));
    }

    // Both spellings compose: only the member neither of them names stays dead.
    #[test]
    fn value_and_type_space_qualified_names_compose() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
    Fries = "fries",
}
export type A = typeof Food.Pizza;
export type B = Food.Taco;
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Fries"));
    }

    // Regression for #8382 — indexing the `typeof` projection with a string
    // literal names exactly one member, so the others stay dead.
    #[test]
    fn typeof_projection_indexed_by_string_literal_names_one_member() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
}
export type P = (typeof Food)['Pizza'];
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Taco"));
    }

    // A union index names each of its literals, and nothing else.
    #[test]
    fn typeof_projection_indexed_by_literal_union_names_each_member() {
        let source = r#"
enum Food {
    Pizza = "pizza",
    Taco = "taco",
    Fries = "fries",
}
export type P = (typeof Food)['Pizza' | 'Taco'];
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Fries"));
    }

    // Regression for #8381 — enums are tracked by the symbol their name binds,
    // so a same-named enum in an inner scope does not hide the outer one's dead
    // members.
    #[test]
    fn same_named_inner_enum_does_not_hide_outer_dead_members() {
        let source = r#"
enum Food {
    Pizza = 'pizza',
    Taco = 'taco',
}
export function local() {
    enum Food {
        Burger = 'burger',
    }
    return Food.Burger;
}
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().any(|d| d.message.contains("Pizza")));
        assert!(diags.iter().any(|d| d.message.contains("Taco")));
    }

    // A local binding that shadows the enum name exempts nothing, in type space
    // as in value space.
    #[test]
    fn shadowing_binding_does_not_exempt_the_enum() {
        let keyof_typeof = r#"
enum Food {
    Pizza = 'pizza',
    Taco = 'taco',
}
export function scope() {
    const Food = { other: 1 };
    type T = keyof typeof Food;
    const t: T = 'other';
    return t;
}
"#;
        assert_eq!(run(keyof_typeof).len(), 2);

        let object_values = r#"
enum Food {
    Pizza = 'pizza',
    Taco = 'taco',
}
export function scope(Food: Record<string, number>) {
    return Object.values(Food);
}
"#;
        assert_eq!(run(object_values).len(), 2);

        let member_access = r#"
enum Food {
    Pizza = 'pizza',
    Taco = 'taco',
}
export function scope() {
    const Food = { Pizza: 1, Taco: 2 };
    return Food.Pizza + Food.Taco;
}
"#;
        assert_eq!(run(member_access).len(), 2);
    }

    // TypeScript merges same-named enum declarations in one scope into a single
    // enum, so their members share one entry and a reference to either
    // declaration's member counts.
    #[test]
    fn merged_enum_declarations_share_one_entry() {
        let source = r#"
enum Food {
    Pizza = 1,
}
enum Food {
    Taco = 2,
}
const p = Food.Pizza;
"#;
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Taco"));
    }

    // An ordinary runtime unit test without type assertions still flags a
    // genuinely dead enum member.
    #[test]
    fn ordinary_unit_test_still_flags_unused() {
        let source = r#"
enum Color {
    Red,
    Green,
    Blue,
}
const x = Color.Red;
const y = Color.Green;
"#;
        let diags = run_at(source, "src/widget.test.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Blue"));
    }
}
