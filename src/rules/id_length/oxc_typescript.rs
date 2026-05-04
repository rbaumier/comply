//! id-length OXC backend.

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
}

/// Extract the binding identifier name from a BindingPattern, if it's
/// a simple BindingIdentifier.
fn binding_name<'a>(pat: &'a BindingPattern<'a>) -> Option<(&'a str, oxc_span::Span)> {
    if let BindingPattern::BindingIdentifier(id) = pat {
        Some((id.name.as_str(), id.span))
    } else {
        None
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::VariableDeclarator,
            AstType::Function,
            AstType::Class,
            AstType::FormalParameter,
            AstType::TSInterfaceDeclaration,
            AstType::TSTypeAliasDeclaration,
            AstType::TSEnumDeclaration,
            AstType::MethodDefinition,
            AstType::ObjectProperty,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let min = ctx.config.threshold("id-length", "min", ctx.lang);
        let exceptions = ctx.config.string_list("id-length", "exceptions", ctx.lang);
        let patterns = compile_patterns(
            &ctx.config.string_list("id-length", "exception_patterns", ctx.lang),
        );

        let names: Vec<(&str, oxc_span::Span)> = match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                // Handles both `const x = ...` and destructuring `const { x } = ...`
                match &decl.id {
                    BindingPattern::BindingIdentifier(id) => {
                        vec![(id.name.as_str(), id.span)]
                    }
                    BindingPattern::ObjectPattern(obj) => {
                        // Shorthand destructuring: `const { x } = ...`
                        obj.properties
                            .iter()
                            .filter_map(|prop| {
                                if prop.shorthand {
                                    binding_name(&prop.value)
                                } else {
                                    None
                                }
                            })
                            .collect()
                    }
                    _ => return,
                }
            }
            AstKind::Function(func) => {
                if let Some(ref id) = func.id {
                    vec![(id.name.as_str(), id.span)]
                } else {
                    return;
                }
            }
            AstKind::Class(class) => {
                if let Some(ref id) = class.id {
                    vec![(id.name.as_str(), id.span)]
                } else {
                    return;
                }
            }
            AstKind::FormalParameter(param) => {
                if let Some((name, span)) = binding_name(&param.pattern) {
                    vec![(name, span)]
                } else {
                    return;
                }
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                vec![(iface.id.name.as_str(), iface.id.span)]
            }
            AstKind::TSTypeAliasDeclaration(alias) => {
                vec![(alias.id.name.as_str(), alias.id.span)]
            }
            AstKind::TSEnumDeclaration(en) => {
                vec![(en.id.name.as_str(), en.id.span)]
            }
            AstKind::MethodDefinition(method) => {
                if let PropertyKey::StaticIdentifier(ref id) = method.key {
                    vec![(id.name.as_str(), id.span)]
                } else {
                    return;
                }
            }
            _ => return,
        };

        for (name, span) in names {
            if name.chars().count() >= min {
                continue;
            }
            if exceptions.iter().any(|e| e == name) {
                continue;
            }
            if patterns.iter().any(|p| p.is_match(name)) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Identifier `{name}` is too short (< {min})."),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
