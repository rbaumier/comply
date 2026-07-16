//! vue-typed-define-props-emits oxc backend.
//!
//! Runs over a parsed `<script setup>` block and flags a `defineProps` /
//! `defineEmits` compiler-macro call written in the runtime object/array form
//! (`defineProps({ ... })`, `defineProps([...])`, `defineEmits([...])`,
//! `defineEmits({ ... })`), which loses the type inference the generic form
//! (`defineProps<{ ... }>()`) provides in a `lang="ts"` SFC.
//!
//! A `defineProps({ ...runtimeProps })` / `defineEmits({ ...x })` whose object
//! argument contains a spread is left alone: it composes an existing runtime
//! props object (defaults, validators, `PropType` casts) that has no type-only
//! equivalent — a spread of a runtime binding cannot be expressed in a generic
//! type parameter, so the runtime form is genuinely required.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectExpression, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["defineProps", "defineEmits"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();
        if name != "defineProps" && name != "defineEmits" {
            return;
        }
        // The type form (`defineProps<{ ... }>()`) carries a type argument, not a
        // runtime one, so it has no first argument to inspect — nothing to flag.
        let Some(first) = call.arguments.first() else {
            return;
        };
        if !is_runtime_form(first) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(semantic.source_text(), call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "In `lang=\"ts\"` SFCs use the type form: `{name}<{{ ... }}>()` instead of the runtime object/array form."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Whether a `defineProps` / `defineEmits` first argument is the runtime
/// object/array form that the type form replaces.
///
/// An object literal that contains a spread (`{ ...runtimeProps }`) is *not*
/// treated as the runtime form: composing a runtime props object cannot be
/// expressed as a type parameter, so it is left alone. An array literal is
/// always the runtime form (its element names, spread or not, drive the
/// diagnostic).
fn is_runtime_form(arg: &Argument) -> bool {
    match arg {
        Argument::ObjectExpression(obj) => !object_has_spread(obj),
        Argument::ArrayExpression(_) => true,
        _ => false,
    }
}

fn object_has_spread(obj: &ObjectExpression) -> bool {
    obj.properties
        .iter()
        .any(|p| matches!(p, ObjectPropertyKind::SpreadProperty(_)))
}
