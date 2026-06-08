//! no-boolean-flag-param OXC backend — flag function parameters typed as boolean.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const PREDICATE_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "was",
];

/// Standard HTML/React controlled-component props that must be boolean.
const ALLOWED_NAMES: &[&str] = &[
    "open", "checked", "disabled", "enabled", "hidden", "required", "selected",
    "readOnly", "multiple", "autoFocus", "autoPlay", "defer", "async",
    "noValidate", "defaultOpen", "defaultChecked",
];

fn has_predicate_prefix(name: &str) -> bool {
    PREDICATE_PREFIXES.iter().any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|rest| {
            rest.is_empty() || rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        })
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FormalParameter(param) = node.kind() else {
            return;
        };

        // Check type annotation is `: boolean`
        let Some(ts_type) = param
            .type_annotation
            .as_ref()
            .map(|ann| &ann.type_annotation)
        else {
            return;
        };

        if !matches!(
            ts_type,
            oxc_ast::ast::TSType::TSBooleanKeyword(_)
        ) {
            return;
        }

        let name = match &param.pattern {
            oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.as_str(),
            _ => "<flag>",
        };

        if ALLOWED_NAMES.contains(&name) || has_predicate_prefix(name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Boolean parameter '{name}' controls a branch — split \
                 into two named functions instead. A ternary or options \
                 object is not a fix; the boolean must disappear from \
                 the signature entirely."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_bare_boolean_param() {
        assert_eq!(run("function send(urgent: boolean) {}").len(), 1);
    }

    #[test]
    fn allows_predicate_prefix() {
        assert!(run("function f(isReady: boolean) {}").is_empty());
        assert!(run("function f(hasAccess: boolean) {}").is_empty());
    }

    // Regression for #272: a `can*` authz-gate flag is predicate-prefixed and
    // exempt — a column factory's `canEdit` must not be flagged (in either the
    // bare or destructured form).
    #[test]
    fn allows_can_prefix_authz_flag() {
        assert!(run("function getTeamsColumns(canEdit: boolean) {}").is_empty());
        assert!(
            run("function getTeamsColumns({ canEdit }: { canEdit: boolean }) {}").is_empty()
        );
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_arrow_function_boolean_param() {
        assert_eq!(run_on("const f = (ready: boolean) => ready;").len(), 1);
    }


    #[test]
    fn allows_non_boolean_params() {
        assert!(run_on("function f(a: number, b: string) {}").is_empty());
    }


    #[test]
    fn allows_boolean_variable_not_in_params() {
        assert!(run_on("const isReady: boolean = true;").is_empty());
    }


    #[test]
    fn allows_controlled_component_props() {
        assert!(run_on("function Dialog(open: boolean) {}").is_empty());
        assert!(run_on("function Input(disabled: boolean) {}").is_empty());
    }


    #[test]
    fn flags_multiple_bare_boolean_params() {
        assert_eq!(run_on("function f(foo: boolean, bar: boolean) {}").len(), 2);
    }
}
