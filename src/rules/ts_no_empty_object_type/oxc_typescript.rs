//! OxcCheck backend for ts-no-empty-object-type — flag `{}` used as a type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

/// True when the empty `{}` is a deliberate type-system idiom rather than a
/// value-level annotation, based on the AST parent of the `TSTypeLiteral`:
///
/// - **Generic constraint / default** (`T extends {}`, `T = {}`): `{}` as a
///   `TSTypeParameter` position means "any non-nullish type" — the standard
///   recursive-accumulator idiom.
/// - **Intersection identity** (`T & {}`): intersecting a non-trivial type with
///   `{}` forces eager type evaluation and is equivalent to `T`. Only exempt when
///   at least one other operand is a non-empty type, so `{} & {}` stays flagged.
fn is_type_system_empty_object_use(parent: &AstKind) -> bool {
    match parent {
        AstKind::TSTypeParameter(_) => true,
        AstKind::TSIntersectionType(intersection) => intersection
            .types
            .iter()
            .any(|ty| !is_empty_object_type(ty)),
        _ => false,
    }
}

fn is_empty_object_type(ty: &TSType) -> bool {
    matches!(ty, TSType::TSTypeLiteral(lit) if lit.members.is_empty())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeLiteral(lit) = node.kind() else { return };
        if !lit.members.is_empty() {
            return;
        }

        let parent = semantic.nodes().parent_node(node.id());

        // Skip `{}` used as a generic constraint/default or an intersection identity.
        if is_type_system_empty_object_use(&parent.kind()) {
            return;
        }

        // Skip `{}` used as an explicit type argument (`Foo<{}>`). Instantiating a
        // generic with `{}` fills a slot the API designed — "empty payload/props" is
        // the intended meaning, not the "matches any non-nullish value" footgun that
        // bites in annotation and declaration positions. Covers React class props
        // (`Component<{}>`), mixin constructors (`Constructor<{}>`), and error/schema
        // factories (`TaggedError("x")<{}>()`).
        if matches!(parent.kind(), AstKind::TSTypeParameterInstantiation(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`{}` as a type matches any non-nullish value. \
                      Use `Record<string, never>` for an empty object, \
                      or `object` / `unknown`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_empty_object_type() {
        assert_eq!(run_on("const x: {} = {};").len(), 1);
    }

    #[test]
    fn allows_tagged_error_empty_type_param() {
        assert!(run_on(r#"export class FooError extends TaggedError("foo")<{}>() {}"#).is_empty());
    }

    /// The `TaggedError(...)` call is often formatted across multiple lines, so
    /// the `{}` type argument lands on a line that has no "TaggedError" text.
    /// Resolving through the call's callee span keeps the exemption working.
    #[test]
    fn allows_tagged_error_empty_type_param_multiline() {
        let src = r#"
            export class OrganizationHasAttachedTeamsError extends TaggedError(
              "organizationHasAttachedTeams",
            )<{}>() {
              override readonly message = "Organization has attached teams";
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_object_as_type_argument() {
        // `{}` as an explicit type argument is a deliberate generic instantiation,
        // not the "any non-nullish" footgun — exempt regardless of the generic.
        assert!(run_on("type X = Map<string, {}>;").is_empty());
        assert!(run_on("class App extends React.Component<{}> {}").is_empty());
        assert!(run_on("function mix<T extends Constructor<{}>>(B: T) {}").is_empty());
        assert!(run_on("const c = new C<{}>();").is_empty());
    }

    #[test]
    fn still_flags_empty_object_in_union() {
        // A union member is not a type-argument position — `{}` stays flagged.
        assert_eq!(run_on("type X = string | {};").len(), 1);
    }

    #[test]
    fn allows_empty_object_in_generic_constraint() {
        assert!(run_on("type Acc<T extends {}> = T;").is_empty());
    }

    #[test]
    fn flags_only_conditional_branch_in_recursive_accumulator() {
        // The `Acc extends {}` constraint and the `IsAny<D, {}, D>` type argument
        // are deliberate type-system positions and exempt; only the bare `{}` in
        // the conditional's else branch (`: {}`) is a value-shaped use and flagged.
        let src = r#"
            type ExtractDispatchFromMiddlewareTuple<
              MiddlewareTuple extends readonly any[],
              Acc extends {},
            > = MiddlewareTuple extends [infer Head, ...infer Tail]
              ? ExtractDispatchFromMiddlewareTuple<
                  Tail,
                  Acc & (Head extends Middleware<infer D> ? IsAny<D, {}, D> : {})
                >
              : Acc
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_intersection_identity_with_mapped_type() {
        assert!(run_on("type Id<T> = { [K in keyof T]: T[K] } & {};").is_empty());
    }

    #[test]
    fn allows_intersection_identity_with_named_type() {
        assert!(run_on("type X = Foo & {};").is_empty());
    }

    #[test]
    fn still_flags_intersection_of_two_empty_objects() {
        assert_eq!(run_on("type X = {} & {};").len(), 2);
    }

    #[test]
    fn still_flags_standalone_empty_object_alias() {
        assert_eq!(run_on("type Empty = {};").len(), 1);
    }

    #[test]
    fn still_flags_empty_object_function_param() {
        assert_eq!(run_on("function f(x: {}) {}").len(), 1);
    }

    // ── #1301 ─────────────────────────────────────────────────────────────

    /// type-fest type-test files use `{}` as the *expected type* under test;
    /// `skip_in_test_dir` suppresses the rule there via the central gate.
    #[test]
    fn skips_expect_type_assertion_in_test_d() {
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "expectType<{}>({} as Schema<{}, number>);",
            "type-fest/test-d/schema.ts",
        );
        assert!(diags.is_empty());
    }

    /// A `{}` default type parameter (`<T = {}>`) means "any non-nullish value"
    /// and is valid in production — the `TSTypeParameter` exemption covers it.
    #[test]
    fn allows_empty_object_default_type_param_in_production() {
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            "function mergeDeep<Options extends MergeDeepOptions = {}>() {}",
            "src/utils.ts",
        );
        assert!(diags.is_empty());
    }

    /// Negative space: a bare `{}` alias in production is still flagged.
    #[test]
    fn still_flags_empty_object_alias_in_production() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "type Foo = {};", "src/utils.ts");
        assert_eq!(diags.len(), 1);
    }

    /// Negative space: a `{}` value annotation in production is still flagged.
    #[test]
    fn still_flags_empty_object_annotation_in_production() {
        let diags =
            crate::rules::test_helpers::run_rule(&Check, "const x: {} = foo;", "src/utils.ts");
        assert_eq!(diags.len(), 1);
    }
}
