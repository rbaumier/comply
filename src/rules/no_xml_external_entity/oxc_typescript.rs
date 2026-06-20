//! no-xml-external-entity OXC backend — flag XML parsers without XXE protection.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind as OxcAstKind;
use oxc_ast::ast::{
    Argument, Expression, ImportDeclarationSpecifier, ObjectPropertyKind, PropertyKey,
};
use std::sync::Arc;

const XML_PARSER_NAMES: &[&str] = &["DOMParser", "XMLParser"];
const XML_PARSER_MODULES: &[&str] = &["xml2js"];

/// Server-side XML packages that export a `DOMParser`/`XMLParser` constructor
/// which can resolve external entities. The browser-global `DOMParser` is
/// XXE-safe by spec (no external-entity option, runs in the sandbox), so a
/// `new DOMParser()` whose binding is *not* imported from one of these packages
/// is never flagged.
const SERVER_XML_PACKAGES: &[&str] =
    &["@xmldom/xmldom", "xmldom", "fast-xml-parser", "libxmljs", "libxmljs2"];

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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
                // The browser-global `DOMParser`/`XMLParser` has no
                // external-entity option and is XXE-safe by spec. Only a
                // constructor imported from a server-side XML package can be
                // vulnerable, so skip any binding that is not such an import.
                if !is_imported_from_server_xml_package(name, semantic) {
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

/// Root package of a bare import specifier: `@scope/pkg/deep` → `@scope/pkg`,
/// `fast-xml-parser/foo` → `fast-xml-parser`.
fn import_root_package(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        let end = specifier
            .match_indices('/')
            .nth(1)
            .map(|(idx, _)| idx)
            .unwrap_or(specifier.len());
        return &specifier[..end];
    }
    specifier.split('/').next().unwrap_or(specifier)
}

/// True when `local_name` is the local binding of an import from a server-side
/// XML package ([`SERVER_XML_PACKAGES`]). Distinguishes an imported, potentially
/// XXE-capable `DOMParser`/`XMLParser` from the XXE-safe browser global of the
/// same name, which has no import declaration.
fn is_imported_from_server_xml_package(
    local_name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    semantic.nodes().iter().any(|node| {
        let OxcAstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        if !SERVER_XML_PACKAGES.contains(&import_root_package(decl.source.value.as_str())) {
            return false;
        }
        let Some(specifiers) = &decl.specifiers else {
            return false;
        };
        specifiers.iter().any(|spec| {
            let local = match spec {
                ImportDeclarationSpecifier::ImportSpecifier(named) => &named.local,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => &def.local,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => &ns.local,
            };
            local.name.as_str() == local_name
        })
    })
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Regression for #5016: the browser-global `DOMParser` is XXE-safe by spec
    // (no external-entity option, runs in the sandbox), so it must not be
    // flagged even though there is no protection argument to set.
    #[test]
    fn allows_browser_global_dom_parser_issue_5016() {
        assert!(run("const doc = new DOMParser().parseFromString(svg, 'image/svg+xml');").is_empty());
    }

    // An imported, server-side `DOMParser` (e.g. @xmldom/xmldom) can resolve
    // external entities and stays flagged.
    #[test]
    fn flags_imported_server_dom_parser() {
        let src = "import { DOMParser } from '@xmldom/xmldom';\nconst doc = new DOMParser().parseFromString(xml);";
        assert_eq!(run(src).len(), 1);
    }

    // fast-xml-parser's `XMLParser` is server-side and XXE-capable.
    #[test]
    fn flags_imported_fast_xml_parser() {
        let src = "import { XMLParser } from 'fast-xml-parser';\nconst p = new XMLParser();";
        assert_eq!(run(src).len(), 1);
    }

    // A default import of `DOMParser` from a server-side package is still a
    // server-side parser.
    #[test]
    fn flags_default_imported_server_dom_parser() {
        let src = "import DOMParser from 'xmldom';\nconst doc = new DOMParser();";
        assert_eq!(run(src).len(), 1);
    }

    // An imported server-side parser with explicit XXE protection is allowed.
    #[test]
    fn allows_imported_parser_with_protection() {
        let src = "import { XMLParser } from 'fast-xml-parser';\nconst p = new XMLParser({ externalEntities: false });";
        assert!(run(src).is_empty());
    }

    // require('xml2js') stays flagged — it is an inherently server-side import.
    #[test]
    fn flags_xml2js_require() {
        assert_eq!(run("const xml2js = require('xml2js');").len(), 1);
    }

    // libxmljs' parseXml resolves external entities by default.
    #[test]
    fn flags_parse_xml_without_protection() {
        assert_eq!(run("const doc = parseXml(input);").len(), 1);
    }
}
