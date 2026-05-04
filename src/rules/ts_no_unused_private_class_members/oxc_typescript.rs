use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

/// Collect all identifier-like text references in a node's subtree by
/// scanning the source slice. This mirrors the tree-sitter approach of
/// collecting all property_identifier / private_property_identifier /
/// identifier texts.
fn collect_text_references<'a>(
    source: &'a str,
    span: oxc_span::Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
    refs: &mut Vec<&'a str>,
) {
    for snode in semantic.nodes().iter() {
        let s = match snode.kind() {
            AstKind::IdentifierReference(id) => {
                if id.span.start >= span.start && id.span.end <= span.end {
                    Some(id.name.as_str())
                } else {
                    None
                }
            }
            AstKind::IdentifierName(id) => {
                if id.span.start >= span.start && id.span.end <= span.end {
                    Some(id.name.as_str())
                } else {
                    None
                }
            }
            AstKind::PrivateIdentifier(id) => {
                if id.span.start >= span.start && id.span.end <= span.end {
                    let full = &source[id.span.start as usize..id.span.end as usize];
                    Some(full)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(name) = s {
            refs.push(name);
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else {
            return;
        };

        // Phase 1: collect private member declarations.
        let mut private_members: HashMap<String, u32> = HashMap::new();

        for element in &class.body.body {
            match element {
                ClassElement::PropertyDefinition(prop) => {
                    let is_private = prop
                        .accessibility
                        .is_some_and(|a| a == TSAccessibility::Private)
                        || matches!(&prop.key, PropertyKey::PrivateIdentifier(_));
                    if !is_private {
                        continue;
                    }
                    let name = match &prop.key {
                        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                        PropertyKey::PrivateIdentifier(id) => {
                            format!("#{}", id.name)
                        }
                        _ => continue,
                    };
                    if name == "constructor" {
                        continue;
                    }
                    private_members.entry(name).or_insert(prop.span.start);
                }
                ClassElement::MethodDefinition(m) => {
                    let is_private = m
                        .accessibility
                        .is_some_and(|a| a == TSAccessibility::Private)
                        || matches!(&m.key, PropertyKey::PrivateIdentifier(_));
                    if !is_private {
                        continue;
                    }
                    let name = match &m.key {
                        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                        PropertyKey::PrivateIdentifier(id) => {
                            format!("#{}", id.name)
                        }
                        _ => continue,
                    };
                    if name == "constructor" {
                        continue;
                    }
                    private_members.entry(name).or_insert(m.span.start);
                }
                _ => {}
            }
        }

        if private_members.is_empty() {
            return;
        }

        // Phase 2: collect all references in the class body.
        let mut all_references: Vec<&str> = Vec::new();
        let _body_span = class.body.span;

        for element in &class.body.body {
            match element {
                ClassElement::PropertyDefinition(prop) => {
                    if let Some(ref value) = prop.value {
                        use oxc_span::GetSpan;
                        let val_span = value.span();
                        collect_text_references(ctx.source, val_span, semantic, &mut all_references);
                    }
                }
                ClassElement::MethodDefinition(m) => {
                    if let Some(ref body) = m.value.body {
                        collect_text_references(ctx.source, body.span, semantic, &mut all_references);
                    }
                }
                _ => {
                    use oxc_span::GetSpan;
                    let elem_span = element.span();
                    collect_text_references(ctx.source, elem_span, semantic, &mut all_references);
                }
            }
        }

        // Phase 3: flag private members with no references.
        // For #private names, the reference text includes the `#`.
        for (name, span_start) in &private_members {
            // For ES private (#foo), references show up as "#foo".
            // For TS private (private foo), references show up as "foo".
            let ref_count = all_references.iter().filter(|r| **r == name.as_str()).count();
            if ref_count == 0 {
                let (line, column) = byte_offset_to_line_col(ctx.source, *span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Private member `{name}` is declared but never used."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
