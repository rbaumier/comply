//! OXC backend for ts-member-ordering — enforce canonical member order in
//! classes and interfaces: signatures, fields, constructors, methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, TSSignature};
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
                let mut max_rank: u8 = 0;
                for elem in &class.body.body {
                    let Some(rank) = class_element_rank(elem) else { continue };
                    if rank < max_rank {
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
