//! unused-enum-member OXC backend — flag TypeScript enum members declared
//! in the current file but never referenced anywhere within that file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
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
    ident: &oxc_ast::ast::IdentifierReference,
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
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

        let mut diagnostics = Vec::new();
        // Map enum_name -> Vec<(member_name, line)>
        let mut enums: FxHashMap<String, Vec<(String, u32)>> = FxHashMap::default();
        // Set of (enum_name, member_name) that are referenced.
        let mut used: FxHashSet<(String, String)> = FxHashSet::default();
        // Track enum node IDs to skip their subtrees in usage collection.
        let mut enum_node_ids: FxHashSet<oxc_semantic::NodeId> = FxHashSet::default();

        // Pass 1: collect enum declarations (non-exported only).
        for node in semantic.nodes().iter() {
            let AstKind::TSEnumDeclaration(decl) = node.kind() else {
                continue;
            };

            // Skip exported enums.
            let nodes = semantic.nodes();
            let parent_id = nodes.parent_id(node.id());
            if parent_id != node.id() {
                let parent = nodes.get_node(parent_id);
                if matches!(parent.kind(), AstKind::ExportNamedDeclaration(_)) {
                    continue;
                }
            }
            // Also check if the source text starts with "export ".
            let decl_text =
                &ctx.source[decl.span.start as usize..decl.span.end as usize];
            if decl_text.starts_with("export ") {
                continue;
            }

            let enum_name = decl.id.name.as_str().to_string();
            let mut members = Vec::new();
            for member in &decl.body.members {
                let member_name =
                    &ctx.source[member.id.span().start as usize..member.id.span().end as usize];
                if member_name.is_empty() {
                    continue;
                }
                let (line, _) =
                    byte_offset_to_line_col(ctx.source, member.span.start as usize);
                members.push((member_name.to_string(), line as u32));
            }
            if !members.is_empty() {
                enums.insert(enum_name, members);
                enum_node_ids.insert(node.id());
            }
        }

        if enums.is_empty() {
            return diagnostics;
        }

        // Pass 2: collect usages (EnumName.MemberName patterns).
        for node in semantic.nodes().iter() {
            // Skip nodes inside enum declarations.
            let mut ancestor_id = node.id();
            let nodes = semantic.nodes();
            let mut skip = false;
            loop {
                if enum_node_ids.contains(&ancestor_id) {
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
                    if let Expression::Identifier(obj) = &member.object {
                        let obj_name = obj.name.as_str();
                        if enums.contains_key(obj_name) {
                            let prop_name = member.property.name.as_str();
                            used.insert((obj_name.to_string(), prop_name.to_string()));
                        }
                    }
                }
                AstKind::ComputedMemberExpression(member) => {
                    if let Expression::Identifier(obj) = &member.object {
                        let obj_name = obj.name.as_str();
                        if enums.contains_key(obj_name)
                            && let Expression::StringLiteral(s) = &member.expression {
                                used.insert((
                                    obj_name.to_string(),
                                    s.value.as_str().to_string(),
                                ));
                            }
                    }
                }
                // `expr in EnumName` reads every member value off the compiled
                // enum object at runtime, so all members are reachable.
                AstKind::BinaryExpression(bin) => {
                    if bin.operator == BinaryOperator::In
                        && let Expression::Identifier(rhs) = &bin.right {
                            let enum_name = rhs.name.as_str();
                            if let Some(members) = enums.get(enum_name) {
                                for (member_name, _) in members {
                                    used.insert((
                                        enum_name.to_string(),
                                        member_name.clone(),
                                    ));
                                }
                            }
                        }
                }
                // A value-position reference to the bare enum identifier consumes
                // the whole enum object at runtime — `Object.values(Food)`,
                // `Object.keys(Food)`, `Object.entries(Food)`, spreading, passing
                // it as an argument, etc. all iterate every member dynamically, so
                // all members are reachable.
                AstKind::IdentifierReference(id) => {
                    let enum_name = id.name.as_str();
                    if let Some(members) = enums.get(enum_name)
                        && is_whole_enum_value_reference(id, node.id(), semantic)
                    {
                        for (member_name, _) in members {
                            used.insert((enum_name.to_string(), member_name.clone()));
                        }
                    }
                }
                _ => {}
            }
        }

        // Diff: flag unused members.
        for (enum_name, members) in &enums {
            for (member_name, line) in members {
                if !used.contains(&(enum_name.clone(), member_name.clone())) {
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
