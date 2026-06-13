//! prefer-reflect-apply oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".apply"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be a member expression with property `apply`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "apply" {
            return;
        }

        // Skip `Reflect.apply(...)`.
        if let Expression::Identifier(obj) = &member.object
            && obj.name.as_str() == "Reflect" {
                return;
            }

        // `Function.prototype.apply(thisArg, argsArray)` takes exactly two
        // arguments. A single-argument `.apply(callback)` is a domain method
        // (Pulumi/SST `Output.apply(cb)`, RxJS, …), not `Function#apply`, so it
        // must not be flagged. Spreads (`fn.apply(...rest)`) are an unknown
        // count and left alone.
        let has_spread = call
            .arguments
            .iter()
            .any(|arg| matches!(arg, oxc_ast::ast::Argument::SpreadElement(_)));
        if call.arguments.len() != 2 || has_spread {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);

        // Check for `Function.prototype.apply.call(…)` pattern by reading source text.
        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text.contains("Function.prototype.apply.call") {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Prefer `Reflect.apply(fn, thisArg, args)` over `Function.prototype.apply.call(fn, thisArg, args)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.".into(),
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

    #[test]
    fn flags_two_arg_apply() {
        assert_eq!(run("fn.apply(null, args);").len(), 1);
    }

    #[test]
    fn flags_two_arg_apply_with_this() {
        assert_eq!(run("foo.bar.apply(this, args);").len(), 1);
    }

    #[test]
    fn allows_reflect_apply() {
        assert!(run("Reflect.apply(fn, null, args);").is_empty());
    }

    #[test]
    fn allows_non_apply_method() {
        assert!(run("fn.call(null, args);").is_empty());
    }

    #[test]
    fn allows_single_arg_apply_callback() {
        // Pulumi/SST `Output.apply(callback)` — a domain method, not `Function#apply`.
        assert!(run("region.apply((region) => bootstrap.forRegion(region));").is_empty());
    }

    #[test]
    fn allows_pulumi_output_apply_chain() {
        // Exact example from issue #1766 (sst/sst function.ts).
        let src = r#"
            const isContainer = all([args.python, dev]).apply(
              ([python, dev]) => !dev && !!python?.container,
            );
            const storage = output(args.storage).apply((v) => v ?? "512 MB");
            const architecture = output(args.architecture).apply((v) => v ?? "x86_64");
            const bootstrapData = region.apply((region) => bootstrap.forRegion(region));
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_zero_arg_apply() {
        assert!(run("obj.apply();").is_empty());
    }

    #[test]
    fn allows_spread_apply() {
        assert!(run("fn.apply(...rest);").is_empty());
    }
}
