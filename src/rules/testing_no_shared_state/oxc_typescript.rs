//! testing-no-shared-state OXC backend — detect program-level `let`/`var`
//! bindings that are assigned inside a `test(...)` / `it(...)` callback,
//! unless a `beforeEach` block exists that also assigns them.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, Expression, VariableDeclarationKind,
};
use oxc_span::GetSpan;
use std::collections::HashSet;
use std::sync::Arc;

/// Mutating array/collection methods.
const MUTATING_METHODS: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
    "set",
    "delete",
    "clear",
    "add",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[] // full-program analysis
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let nodes = semantic.nodes();

        // Step 1: collect program-level let/var bindings.
        // Program-scope variable declarations are direct children of Program.
        let _root_scope = semantic.scoping().root_scope_id();
        let mut bindings: Vec<(String, u32)> = Vec::new(); // (name, span_start)
        let mut names: HashSet<String> = HashSet::new();

        for node in nodes.iter() {
            let AstKind::VariableDeclaration(decl) = node.kind() else {
                continue;
            };
            // Must be at program scope
            if !is_at_program_level(nodes, node.id()) {
                continue;
            }
            let is_let = decl.kind == VariableDeclarationKind::Let;
            let is_var = decl.kind == VariableDeclarationKind::Var;
            if !is_let && !is_var {
                continue;
            }
            for declarator in &decl.declarations {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &declarator.id {
                    let name = ident.name.to_string();
                    bindings.push((name.clone(), declarator.span.start));
                    names.insert(name);
                }
            }
        }

        if bindings.is_empty() {
            return Vec::new();
        }

        // Step 2: find test/it and beforeEach calls, check for mutations.
        let mut mutated_in_tests: HashSet<String> = HashSet::new();
        let mut reset_in_before_each: HashSet<String> = HashSet::new();

        for node in nodes.iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            let callee_name = callee.name.as_str();
            let is_test = callee_name == "test" || callee_name == "it";
            let is_before_each = callee_name == "beforeEach";
            if !is_test && !is_before_each {
                continue;
            }

            // Find the callback argument (arrow or function expression)
            let call_span = call.span;
            let target_set = if is_test {
                &mut mutated_in_tests
            } else {
                &mut reset_in_before_each
            };

            // Scan all nodes within this call's span for mutations
            collect_mutations_in_span(nodes, call_span, &names, target_set);
        }

        if mutated_in_tests.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (name, span_start) in &bindings {
            if !mutated_in_tests.contains(name) {
                continue;
            }
            if reset_in_before_each.contains(name) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, *span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Top-level '{name}' is mutated inside test() without being reset in beforeEach — tests become order-dependent."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn is_at_program_level(nodes: &oxc_semantic::AstNodes, node_id: oxc_semantic::NodeId) -> bool {
    for kind in nodes.ancestor_kinds(node_id).skip(1) {
        match kind {
            AstKind::Program(_) => return true,
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => continue,
            _ => return false,
        }
    }
    false
}

fn collect_mutations_in_span(
    nodes: &oxc_semantic::AstNodes,
    span: oxc_span::Span,
    names: &HashSet<String>,
    found: &mut HashSet<String>,
) {
    for node in nodes.iter() {
        let node_span = node.kind().span();
        if node_span.start < span.start || node_span.end > span.end {
            continue;
        }
        match node.kind() {
            // Case 1 & 2: assignment expressions
            AstKind::AssignmentExpression(assign) => {
                if let Some(name) = root_name_of_target(&assign.left)
                    && names.contains(name) {
                        found.insert(name.to_string());
                    }
            }
            // Case 3: mutating method calls
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    continue;
                };
                let method = member.property.name.as_str();
                if !MUTATING_METHODS.contains(&method) {
                    continue;
                }
                let Expression::Identifier(obj) = &member.object else {
                    continue;
                };
                let obj_name = obj.name.as_str();
                if names.contains(obj_name) {
                    found.insert(obj_name.to_string());
                }
            }
            _ => {}
        }
    }
}

/// Extract the root identifier name from an assignment target.
fn root_name_of_target<'a>(target: &'a AssignmentTarget<'a>) -> Option<&'a str> {
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(ident) => Some(ident.name.as_str()),
        AssignmentTarget::StaticMemberExpression(member) => {
            root_name_of_expr(&member.object)
        }
        AssignmentTarget::ComputedMemberExpression(member) => {
            root_name_of_expr(&member.object)
        }
        _ => None,
    }
}

fn root_name_of_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}
