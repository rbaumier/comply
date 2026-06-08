//! OXC backend for react-no-boolean-variant-props.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use std::sync::Arc;

pub struct Check;

/// Independent observable state flags: a single bit of truth that can hold
/// simultaneously with the others (a form is dirty AND submitting AND
/// disabled). These are NOT mutually-exclusive variants and must not be
/// collapsed into a union. Style/intent variants (`isPrimary`, `isGhost`) and
/// request-status flags (`isLoading`, `isError`, `isSuccess`) are deliberately
/// absent — collapsing *those* is the boolean-blindness smell the rule targets.
const INDEPENDENT_OBSERVABLE_FLAGS: &[&str] = &[
    "Dirty", "Submitting", "Submitted", "Saving", "Saved", "Editing", "Open",
    "Opened", "Closed", "Visible", "Hidden", "Valid", "Invalid", "Checked",
    "Unchecked", "Selected", "Deselected", "Disabled", "Enabled", "Active",
    "Inactive", "Focused", "Blurred", "Touched", "Untouched", "Expanded",
    "Collapsed", "Hovered", "Pressed", "Dragging", "Animating", "ReadOnly",
    "Required", "Optional", "Mounted", "Ready", "Deleting",
];

fn looks_like_variant_prop(name: &str) -> bool {
    for prefix in ["is", "has"] {
        if let Some(rest) = name.strip_prefix(prefix)
            && rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        {
            return !INDEPENDENT_OBSERVABLE_FLAGS.contains(&rest);
        }
    }
    false
}

fn function_name_is_component(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn count_boolean_variants(pattern: &oxc_ast::ast::ObjectPattern) -> usize {
    let mut count = 0usize;
    for prop in &pattern.properties {
        let name_str: Option<String> = if prop.shorthand {
            match &prop.value {
                BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
                BindingPattern::AssignmentPattern(assign) => match &assign.left {
                    BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
                    _ => None,
                },
                _ => None,
            }
        } else {
            prop.key.static_name().map(|s| s.to_string())
        };
        if let Some(ref n) = name_str
            && looks_like_variant_prop(n) {
                count += 1;
            }
    }
    count
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let is_component = match node.kind() {
            AstKind::Function(func) => {
                func.id
                    .as_ref()
                    .is_some_and(|id| function_name_is_component(id.name.as_str()))
            }
            AstKind::ArrowFunctionExpression(_) => {
                let parent = semantic.nodes().parent_node(node.id());
                let AstKind::VariableDeclarator(decl) = parent.kind() else {
                    return;
                };
                match &decl.id {
                    BindingPattern::BindingIdentifier(id) => {
                        function_name_is_component(id.name.as_str())
                    }
                    _ => false,
                }
            }
            _ => return,
        };
        if !is_component {
            return;
        }

        let params = match node.kind() {
            AstKind::Function(func) => &func.params,
            AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
            _ => return,
        };
        let Some(first_param) = params.items.first() else {
            return;
        };

        let object_pattern = match &first_param.pattern {
            BindingPattern::ObjectPattern(pat) => Some(pat.as_ref()),
            BindingPattern::AssignmentPattern(assign) => match &assign.left {
                BindingPattern::ObjectPattern(pat) => Some(pat.as_ref()),
                _ => None,
            },
            _ => None,
        };
        let Some(pattern) = object_pattern else {
            return;
        };

        let count = count_boolean_variants(pattern);
        if count < 2 {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, pattern.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{count} boolean variant props on this component — collapse into a single \
                 `variant: '...' | '...'` union to eliminate mutually-exclusive invalid states."
            ),
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_two_style_variants() {
        let src = r#"function Button({ isPrimary, isGhost }) { return <button />; }"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn flags_status_family() {
        let src = r#"function Status({ isLoading, isError, isSuccess }) { return <div />; }"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #281: independent observable form/UI flags coexist by
    // design — they are not mutually-exclusive variants, so collapsing them
    // into a union would be wrong. `submitDisabled` lacks an is/has prefix and
    // was never counted; `isDirty`/`isSubmitting` are observables.
    #[test]
    fn allows_independent_observable_flags() {
        let src = r#"function Form({ isDirty, isSubmitting, submitDisabled }) { return <form />; }"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_observables_mixed_with_single_variant() {
        // One real variant + observables → below the 2-variant threshold.
        let src = r#"function Panel({ isPrimary, isOpen, isValid }) { return <div />; }"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
