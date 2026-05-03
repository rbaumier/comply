//! better-result-constructor-spreads-args OXC backend — flag TaggedError
//! constructors where super() doesn't spread args.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, FormalParameterKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn extends_tagged_error(source: &str, class: &oxc_ast::ast::Class) -> bool {
    let Some(super_class) = &class.super_class else { return false };
    let text = &source[super_class.span().start as usize..super_class.span().end as usize];
    text.contains("TaggedError")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["TaggedError"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };
        if !extends_tagged_error(ctx.source, class) {
            return;
        }

        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else { continue };
            // Must be a constructor
            let is_ctor = match &method.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str() == "constructor",
                _ => false,
            };
            if !is_ctor {
                continue;
            }
            // Skip if constructor has no parameters
            let params = &method.value.params;
            if params.kind == FormalParameterKind::FormalParameter && params.items.is_empty() {
                continue;
            }
            if params.items.is_empty() {
                continue;
            }
            // Find super() call in the constructor body source text
            let Some(body) = &method.value.body else { continue };
            let body_text = &ctx.source[body.span.start as usize..body.span.end as usize];
            // Find super(...) call
            if let Some(super_pos) = body_text.find("super(") {
                let super_start = body.span.start as usize + super_pos;
                // Find the matching closing paren
                let after_super = &ctx.source[super_start..];
                if let Some(open) = after_super.find('(') {
                    let args_start = super_start + open;
                    let mut depth = 0;
                    let mut args_end = args_start;
                    for (i, ch) in after_super[open..].char_indices() {
                        match ch {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    args_end = super_start + open + i + 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let args_text = &ctx.source[args_start..args_end];
                    if !args_text.contains("...") {
                        let (line, column) = byte_offset_to_line_col(ctx.source, super_start);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "TaggedError constructor super() must spread args (e.g. `super({ ...args, message })`)."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }
    }
}
