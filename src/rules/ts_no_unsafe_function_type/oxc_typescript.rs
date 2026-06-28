//! ts-no-unsafe-function-type oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSTypeName;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeReference]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // tsd type-test files pass `Function` as an input to the utility under
        // test (e.g. `ConditionalSimplify<SomeFunction, Function>`), so the
        // banned type is the test subject, not application code.
        if ctx.file.is_type_test_file() {
            return;
        }
        let AstKind::TSTypeReference(type_ref) = node.kind() else {
            return;
        };
        let name = match &type_ref.type_name {
            TSTypeName::IdentifierReference(id) => id.name.as_str(),
            _ => return,
        };
        if name != "Function" {
            return;
        }
        let nodes = semantic.nodes();
        // `T extends Function ? A : B`: in a conditional type the `extends`
        // operand is the idiomatic "is this callable?" predicate, not a
        // value-type annotation, and has no narrower replacement in that
        // position. Exempt only that constraint slot (the type to the right of
        // `extends`); `Function` in the branches or other annotations still flags.
        if let AstKind::TSConditionalType(cond) = nodes.parent_kind(node.id())
            && cond.extends_type.span() == type_ref.span
        {
            return;
        }
        // `value is Function` / `asserts value is Function`: when `Function` is
        // the narrowed type of a type predicate it asserts only that the value is
        // callable with an unknown signature — no concrete call signature like
        // `() => void` can replace it without over-narrowing the claim. The
        // predicate wraps its type in a `TSTypeAnnotation`, so the direct slot is
        // parent = `TSTypeAnnotation`, grandparent = `TSTypePredicate`; `Function`
        // nested deeper (e.g. `value is Function[]`) stays flagged.
        if matches!(nodes.parent_kind(node.id()), AstKind::TSTypeAnnotation(_))
            && matches!(
                nodes.parent_kind(nodes.parent_id(node.id())),
                AstKind::TSTypePredicate(_)
            )
        {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, type_ref.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Built-in `Function` type loses signature info — replace with \
                      a precise call signature like `(arg: T) => U`."
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(
            std::path::Path::new(path),
            source,
            crate::files::Language::TypeScript,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
    }

    #[test]
    fn flags_function_type_annotation() {
        let src = "function call(cb: Function) { cb(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_typed_callback() {
        let src = "function call(cb: () => void) { cb(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn exempts_tsd_type_test_file_issue3324() {
        // type-fest test-d/conditional-simplify.ts: `Function` is the input to
        // the utility under test, not application code.
        let src = "type SimplifiedFunctionPass = ConditionalSimplify<SomeFunction, Function>;";
        assert!(run_at(src, "test-d/conditional-simplify.ts").is_empty());
    }

    #[test]
    fn still_flags_function_type_in_production_issue3324() {
        assert_eq!(run_at("function call(cb: Function) { cb(); }", "src/widget.ts").len(), 1);
    }

    #[test]
    fn exempts_function_in_type_predicate_issue6494() {
        // sindresorhus/is source/index.ts:534 — `value is Function` asserts the
        // value is callable with an unknown signature; no narrower type fits.
        let src = "function isFunction(value: unknown): value is Function { return typeof value === 'function'; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn exempts_function_in_asserts_type_predicate_issue6494() {
        let src = "function assertFunction(value: unknown): asserts value is Function {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_function_in_parameter_return_and_variable_issue6494() {
        // Only the type-predicate narrowed-type slot is exempt; ordinary
        // value-type positions still flag.
        assert_eq!(run("function f(cb: Function) {}").len(), 1);
        assert_eq!(run("function g(): Function { return () => {}; }").len(), 1);
        assert_eq!(run("const h: Function = () => {};").len(), 1);
    }

    #[test]
    fn still_flags_function_nested_in_type_predicate_issue6494() {
        // Only the direct narrowed-type slot is exempt; `Function` nested inside
        // the narrowed type (here an array element) keeps a narrower replacement.
        let src = "function isFns(v: unknown): v is Function[] { return Array.isArray(v); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn exempts_function_in_conditional_extends_operand_issue6533() {
        // unjs/defu src/types.ts:81,89 — `Merge<>` conditional type. Both
        // `extends Function` sites are the idiomatic "is callable?" predicate
        // with no narrower replacement, so neither flags.
        let src = "export type Merge<Destination, Defaults> = Destination extends Function \
                   ? Destination | Defaults \
                   : Defaults extends Function \
                   ? Destination | Defaults \
                   : MergeObjects<Destination, Defaults>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_function_inside_conditional_extends_array_issue6533() {
        // Only the bare `extends Function` constraint slot is exempt; `Function`
        // nested in an array within the `extends` operand has a narrower
        // replacement, so it flags.
        assert_eq!(run("type T<U> = U extends Function[] ? U : never;").len(), 1);
    }

    #[test]
    fn still_flags_function_in_conditional_branch_issue6533() {
        // `Function` in the true/false branch of a conditional type is a
        // value-type annotation, not the constraint predicate — still flags.
        assert_eq!(run("type T<U> = U extends string ? Function : never;").len(), 1);
    }

    #[test]
    fn still_flags_value_level_function_annotation_issue6533() {
        // Negative control: ordinary value-level annotations still flag.
        assert_eq!(run("const fn: Function = () => {};").len(), 1);
        assert_eq!(run("function f(cb: Function) {}").len(), 1);
    }
}
