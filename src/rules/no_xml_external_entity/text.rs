use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const XML_PARSER_PATTERNS: &[&str] = &[
    "DOMParser(",
    "xml2js",
    "parseXml(",
    "XMLParser(",
];

const XXE_PROTECTIONS: &[&str] = &[
    "noent: false",
    "noent:false",
    "externalEntities: false",
    "externalEntities:false",
];

/// Returns true if any of the protection patterns appear in the line.
fn has_xxe_protection(line: &str) -> bool {
    XXE_PROTECTIONS.iter().any(|p| line.contains(p))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let has_parser = XML_PARSER_PATTERNS.iter().any(|p| line.contains(p));
            if !has_parser {
                continue;
            }

            // Check current line and adjacent lines (prev/next) for protection.
            let protected = has_xxe_protection(line)
                || (idx > 0 && has_xxe_protection(lines[idx - 1]))
                || (idx + 1 < lines.len() && has_xxe_protection(lines[idx + 1]));

            if !protected {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-xml-external-entity".into(),
                    message: "XML parser without XXE protection — set `noent: false` or `externalEntities: false`.".into(),
                    severity: Severity::Error,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_dom_parser() {
        assert_eq!(run("const parser = new DOMParser();").len(), 1);
    }

    #[test]
    fn flags_xml2js() {
        assert_eq!(run("const parser = require('xml2js');").len(), 1);
    }

    #[test]
    fn flags_xml_parser() {
        assert_eq!(run("const p = new XMLParser();").len(), 1);
    }

    #[test]
    fn allows_dom_parser_with_protection_on_next_line() {
        let src = "const parser = new DOMParser();\nparser.noent: false;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_xml_parser_with_protection_same_line() {
        assert!(run("new XMLParser({ noent: false });").is_empty());
    }

    #[test]
    fn allows_external_entities_false() {
        let src = "externalEntities: false,\nconst p = new XMLParser();";
        assert!(run(src).is_empty());
    }
}
