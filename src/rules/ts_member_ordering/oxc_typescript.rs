//! OXC backend for ts-member-ordering — enforce canonical member order in
//! classes and interfaces: signatures, fields, constructors, methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, Class, ClassElement, Expression, MethodDefinitionKind, Statement,
    TSSignature,
};
use rustc_hash::FxHashSet;
use std::sync::Arc;

fn class_element_rank(elem: &ClassElement) -> Option<u8> {
    match elem {
        ClassElement::TSIndexSignature(_) => Some(0),
        ClassElement::PropertyDefinition(prop) => {
            if prop.r#type == oxc_ast::ast::PropertyDefinitionType::TSAbstractPropertyDefinition {
                Some(1)
            } else {
                Some(1)
            }
        }
        ClassElement::MethodDefinition(method) => {
            if method.kind == oxc_ast::ast::MethodDefinitionKind::Constructor {
                Some(2)
            } else {
                Some(3)
            }
        }
        ClassElement::AccessorProperty(_) => Some(1),
        ClassElement::StaticBlock(_) => None,
    }
}

/// Property names assigned via `this.<name> = ...` directly in the constructor
/// body. Such fields are legitimately declared after methods: the constructor
/// definitely assigns them, so their declaration position is purely stylistic
/// (e.g. AutoRest-generated SDK clients group sub-client fields at the end).
fn constructor_assigned_fields<'a>(class: &Class<'a>) -> FxHashSet<&'a str> {
    let mut names = FxHashSet::default();
    for elem in &class.body.body {
        let ClassElement::MethodDefinition(method) = elem else { continue };
        if method.kind != MethodDefinitionKind::Constructor || method.r#static {
            continue;
        }
        let Some(body) = method.value.body.as_ref() else { continue };
        for stmt in &body.statements {
            let Statement::ExpressionStatement(expr_stmt) = stmt else { continue };
            let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
                continue;
            };
            let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
                continue;
            };
            if !matches!(&member.object, Expression::ThisExpression(_)) {
                continue;
            }
            names.insert(member.property.name.as_str());
        }
    }
    names
}

fn ts_signature_rank(sig: &TSSignature) -> Option<u8> {
    match sig {
        TSSignature::TSIndexSignature(_)
        | TSSignature::TSCallSignatureDeclaration(_)
        | TSSignature::TSConstructSignatureDeclaration(_) => Some(0),
        TSSignature::TSPropertySignature(_) => Some(1),
        TSSignature::TSMethodSignature(_) => Some(3),
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class, AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Class(class) => {
                let assigned = constructor_assigned_fields(class);
                let mut max_rank: u8 = 0;
                for elem in &class.body.body {
                    let Some(rank) = class_element_rank(elem) else { continue };
                    if rank < max_rank {
                        if let ClassElement::PropertyDefinition(prop) = elem
                            && prop
                                .key
                                .name()
                                .is_some_and(|name| assigned.contains(name.as_ref()))
                        {
                            continue;
                        }
                        let span = match elem {
                            ClassElement::MethodDefinition(m) => m.span,
                            ClassElement::PropertyDefinition(p) => p.span,
                            ClassElement::AccessorProperty(a) => a.span,
                            ClassElement::TSIndexSignature(s) => s.span,
                            ClassElement::StaticBlock(s) => s.span,
                        };
                        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Member is out of order — expected: signatures, \
                                      fields, constructors, methods."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    } else {
                        max_rank = rank;
                    }
                }
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                let mut max_rank: u8 = 0;
                for sig in &iface.body.body {
                    let Some(rank) = ts_signature_rank(sig) else { continue };
                    if rank < max_rank {
                        let span = match sig {
                            TSSignature::TSIndexSignature(s) => s.span,
                            TSSignature::TSCallSignatureDeclaration(s) => s.span,
                            TSSignature::TSConstructSignatureDeclaration(s) => s.span,
                            TSSignature::TSPropertySignature(s) => s.span,
                            TSSignature::TSMethodSignature(s) => s.span,
                        };
                        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Member is out of order — expected: signatures, \
                                      fields, constructors, methods."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    } else {
                        max_rank = rank;
                    }
                }
            }
            _ => {}
        }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_field_after_method() {
        let diags = run("class Foo {\n  bar(): void {}\n  x: string;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_constructor_assigned_fields_after_methods() {
        // AutoRest-generated ARM SDK pattern: sub-client fields are declared
        // after methods but assigned in the constructor body.
        let src = "export class ManagementGroupsAPI {\n\
            \x20 $host: string;\n\
            \x20 apiVersion: string;\n\
            \x20 constructor() {\n\
            \x20   this.managementGroups = new ManagementGroupsImpl(this);\n\
            \x20   this.entities = new EntitiesImpl(this);\n\
            \x20 }\n\
            \x20 checkNameAvailability(): Promise<void> { return Promise.resolve(); }\n\
            \x20 startTenantBackfill(): Promise<void> { return Promise.resolve(); }\n\
            \x20 managementGroups: ManagementGroups;\n\
            \x20 entities: Entities;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_unassigned_field_after_method() {
        // A field declared after methods that is NOT assigned in the
        // constructor is a genuine ordering smell and must still fire.
        let src = "class Foo {\n\
            \x20 constructor() {\n\
            \x20   this.assigned = 1;\n\
            \x20 }\n\
            \x20 bar(): void {}\n\
            \x20 assigned: number;\n\
            \x20 stray: string;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
