//! boolean-naming OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

const VALID_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "was", "in", "seen", "found",
];
const NEGATIVE_SUBSTRINGS: &[&str] = &["Not", "Isnt", "Cannot", "Cant", "Shouldnt"];

/// Standard HTML attributes and React controlled-component props whose names
/// are dictated by the platform / component library API.
const ALLOWED_NAMES: &[&str] = &[
    "open", "checked", "disabled", "enabled", "hidden", "required", "selected",
    "readOnly", "multiple", "autoFocus", "autoPlay", "defer", "async",
    "noValidate", "value", "defaultOpen", "defaultChecked",
];

/// Return a short problem description if the name doesn't match the rule.
fn classify_name(name: &str) -> Option<&'static str> {
    if NEGATIVE_SUBSTRINGS.iter().any(|neg| name.contains(neg)) {
        return Some("is negatively phrased — use the positive form with `!`");
    }
    for &prefix in VALID_PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix)
            && (rest.is_empty() || rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())) {
                return None;
            }
    }
    Some("is missing a predicate prefix")
}

/// Check if a type annotation is `: boolean`.
fn is_boolean_annotation(annotation: &TSTypeAnnotation) -> bool {
    matches!(&annotation.type_annotation, TSType::TSBooleanKeyword(_))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (name, span, is_bool) = match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                let BindingPattern::BindingIdentifier(ref id) = decl.id else {
                    return;
                };
                let name = id.name.as_str();

                // Check for `: boolean` annotation
                let has_annotation = decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| is_boolean_annotation(ann));

                // Check for `= true` / `= false` initializer
                let has_bool_init = decl.init.as_ref().is_some_and(|init| {
                    matches!(init, Expression::BooleanLiteral(_))
                });

                if !has_annotation && !has_bool_init {
                    return;
                }
                (name, id.span, true)
            }
            AstKind::FormalParameter(param) => {
                let BindingPattern::BindingIdentifier(ref id) = param.pattern else {
                    return;
                };
                let name = id.name.as_str();
                let has_annotation = param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| is_boolean_annotation(ann));
                if !has_annotation {
                    return;
                }
                (name, id.span, true)
            }
            _ => return,
        };

        if !is_bool {
            return;
        }

        if ALLOWED_NAMES.contains(&name) {
            return;
        }

        let Some(problem) = classify_name(name) else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Boolean '{name}' {problem}. Use a predicate prefix: \
                 `is*`, `has*`, `should*`, `can*`, `will*`, `did*`, `was*`, \
                 `in*`, `seen*`, `found*`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
