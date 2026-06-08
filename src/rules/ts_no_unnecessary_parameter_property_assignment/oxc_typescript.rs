//! OXC backend for ts-no-unnecessary-parameter-property-assignment.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, BindingPattern, Expression, MethodDefinitionKind,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else {
            return;
        };

        // Left side must be `this.something` (static member).
        let AssignmentTarget::StaticMemberExpression(left_member) = &assign.left else {
            return;
        };
        let Expression::ThisExpression(_) = &left_member.object else {
            return;
        };
        let prop_name = left_member.property.name.as_str();

        // Right side must be a simple identifier matching the property name.
        let Expression::Identifier(right_id) = &assign.right else {
            return;
        };
        if right_id.name.as_str() != prop_name {
            return;
        }

        // Walk up to find the enclosing constructor method.
        let mut current_id = node.id();
        let mut ctor_node_id = None;
        loop {
            let parent = semantic.nodes().parent_node(current_id);
            match parent.kind() {
                AstKind::MethodDefinition(method) => {
                    if method.kind == MethodDefinitionKind::Constructor {
                        ctor_node_id = Some(parent.id());
                    }
                    break;
                }
                // Don't cross function boundaries.
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => break,
                AstKind::Program(_) => break,
                _ => {
                    current_id = parent.id();
                }
            }
        }

        let Some(_ctor_id) = ctor_node_id else {
            return;
        };

        // Now find the constructor's Function to inspect its params.
        // Walk up from assignment to find the Function that is the constructor body.
        let mut current_id = node.id();
        let mut ctor_params = None;
        loop {
            let parent = semantic.nodes().parent_node(current_id);
            match parent.kind() {
                AstKind::Function(func) => {
                    ctor_params = Some(&func.params);
                    break;
                }
                AstKind::Program(_) => break,
                _ => {
                    current_id = parent.id();
                }
            }
        }

        let Some(params) = ctor_params else {
            return;
        };

        // Check if the matching parameter is a parameter property (has accessibility modifier or readonly).
        let is_param_property = params.items.iter().any(|param| {
            let has_modifier = param.accessibility.is_some() || param.readonly;
            if !has_modifier {
                return false;
            }
            match &param.pattern {
                BindingPattern::BindingIdentifier(id) => id.name.as_str() == prop_name,
                _ => false,
            }
        });

        if !is_param_property {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "This assignment is unnecessary — the parameter property already assigns it."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn allows_different_property() {
        let src = r#"
class Foo {
    constructor(public name: string) {
        this.label = name;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_non_parameter_property() {
        let src = r#"
class Foo {
    constructor(name: string) {
        this.name = name;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
