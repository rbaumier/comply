//! ts-no-restricted-types OXC backend.
//!
//! Flags banned types (`Function`) in type annotation positions by scanning
//! all TSTypeReference nodes in the semantic tree. Wrapper object types
//! (`Object`, `String`, `Number`, `Boolean`, `Symbol`, `BigInt`) are owned by
//! `ts-no-wrapper-object-types` and intentionally excluded here to avoid
//! duplicate diagnostics on the same type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Banned type names and replacement messages.
const BANNED_TYPES: &[(&str, &str)] = &[(
    "Function",
    "Use a specific function type like `() => void` instead of `Function`.",
)];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // tsd type-test files pass banned types as inputs to the utility under
        // test (e.g. `ConditionalSimplify<SomeFunction, Function>`), so the
        // banned type is the test subject, not application code.
        if ctx.file.is_type_test_file() {
            return diagnostics;
        }

        for node in semantic.nodes().iter() {
            match node.kind() {
                // TSTypeReference with a single identifier name matching banned types.
                AstKind::TSTypeReference(type_ref) => {
                    let name = type_ref.type_name.to_string();
                    if let Some(&(_, msg)) = BANNED_TYPES.iter().find(|&&(t, _)| t == name.as_str())
                    {
                        // `Function` as the `extends` operand of a conditional
                        // type (`T extends Function ? A : B`) is the idiomatic
                        // "is this callable?" predicate, not a value-type
                        // annotation, and has no narrower replacement in that
                        // position. Exempt only that constraint slot; `Function`
                        // in parameter/return/property annotations still flags.
                        if let AstKind::TSConditionalType(cond) =
                            semantic.nodes().parent_node(node.id()).kind()
                            && cond.extends_type.span() == type_ref.span
                        {
                            continue;
                        }
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, type_ref.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "ts-no-restricted-types".into(),
                            message: msg.to_string(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
    fn flags_function_type() {
        let d = run_on("const f: Function = () => {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Function"));
    }

    #[test]
    fn ignores_object_wrapper_type() {
        // `Object` is owned by ts-no-wrapper-object-types; this rule must not
        // also flag it (regression for #1222).
        assert!(run_on("const o: Object = {};").is_empty());
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
        assert_eq!(run_at("const f: Function = () => {};", "src/widget.ts").len(), 1);
    }

    #[test]
    fn exempts_function_in_conditional_extends_operand_issue6137() {
        // honojs/hono src/context.ts:78 and src/utils/types.ts:98 — `extends
        // Function` is the idiomatic "is callable?" predicate, not a value-type
        // annotation, and has no narrower replacement in that position.
        assert!(
            run_on(
                "export type Renderer = ContextRenderer extends Function ? ContextRenderer : DefaultRenderer;"
            )
            .is_empty()
        );
        assert!(
            run_on(
                "export type InterfaceToType<T> = T extends Function ? T : { [K in keyof T]: InterfaceToType<T[K]> };"
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_function_in_parameter_position_issue6137() {
        assert_eq!(run_on("function call(fn: Function) { return fn(); }").len(), 1);
    }

    #[test]
    fn still_flags_function_in_return_position_issue6137() {
        assert_eq!(run_on("function make(): Function { return () => {}; }").len(), 1);
    }

    #[test]
    fn still_flags_function_inside_conditional_extends_array_issue6137() {
        // Only the bare `extends Function` constraint slot is exempt; `Function`
        // nested inside another type (here an array) in the `extends` operand is
        // still a concrete usage with a narrower replacement, so it flags.
        assert_eq!(run_on("type T<U> = U extends Function[] ? U : never;").len(), 1);
    }

    #[test]
    fn still_flags_function_in_conditional_branch_issue6137() {
        // `Function` in the true/false branch of a conditional type is a
        // value-type annotation, not the constraint predicate — still flags.
        assert_eq!(run_on("type T<U> = U extends string ? Function : never;").len(), 1);
    }
}
