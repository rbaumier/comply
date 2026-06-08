//! no-xml-external-entity OXC backend — flag XML parsers without XXE protection.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

const XML_PARSER_NAMES: &[&str] = &["DOMParser", "XMLParser"];
const XML_PARSER_MODULES: &[&str] = &["xml2js"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["DOMParser", "XMLParser", "parseXml", "xml2js"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (is_xml_parser, span_start) = match node.kind() {
            AstKind::NewExpression(new_expr) => {
                let name = match &new_expr.callee {
                    Expression::Identifier(id) => id.name.as_str(),
                    _ => return,
                };
                if !XML_PARSER_NAMES.contains(&name) {
                    return;
                }
                if has_protection(&new_expr.arguments) {
                    return;
                }
                (true, new_expr.span.start)
            }
            AstKind::CallExpression(call) => {
                let name = match &call.callee {
                    Expression::Identifier(id) => id.name.as_str(),
                    _ => return,
                };
                if name == "require" {
                    let Some(first) = call.arguments.first() else {
                        return;
                    };
                    let Argument::StringLiteral(lit) = first else {
                        return;
                    };
                    if !XML_PARSER_MODULES.contains(&lit.value.as_str()) {
                        return;
                    }
                    (true, call.span.start)
                } else if name == "parseXml" {
                    if has_protection(&call.arguments) {
                        return;
                    }
                    (true, call.span.start)
                } else {
                    return;
                }
            }
            _ => return,
        };

        if !is_xml_parser {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "XML parser without XXE protection — set `noent: false` or `externalEntities: false`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Check whether any argument is an object literal containing `noent: false`
/// or `externalEntities: false`.
fn has_protection(args: &[Argument]) -> bool {
    for arg in args {
        let Argument::ObjectExpression(obj) = arg else {
            continue;
        };
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            let key_name = match &p.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                _ => continue,
            };
            if (key_name == "noent" || key_name == "externalEntities")
                && matches!(&p.value, Expression::BooleanLiteral(b) if !b.value)
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_dom_parser() {
        assert_eq!(run_on("const parser = new DOMParser();").len(), 1);
    }


    #[test]
    fn flags_xml2js_require() {
        assert_eq!(run_on("const parser = require('xml2js');").len(), 1);
    }


    #[test]
    fn flags_xml_parser() {
        assert_eq!(run_on("const p = new XMLParser();").len(), 1);
    }


    #[test]
    fn allows_xml_parser_with_protection() {
        assert!(run_on("new XMLParser({ noent: false });").is_empty());
    }


    #[test]
    fn allows_external_entities_false() {
        assert!(run_on("new XMLParser({ externalEntities: false });").is_empty());
    }
}
