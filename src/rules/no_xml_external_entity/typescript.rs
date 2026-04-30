//! no-xml-external-entity backend — flag XML parsers without XXE protection.

use crate::diagnostic::{Diagnostic, Severity};

const XML_PARSER_NAMES: &[&str] = &["DOMParser", "XMLParser"];
const XML_PARSER_MODULES: &[&str] = &["xml2js"];

/// Check whether an object literal (arguments list) contains an XXE-protection
/// property like `noent: false` or `externalEntities: false`.
fn has_protection(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk all descendants looking for a pair with a protection key.
    let mut cursor = node.walk();
    let mut depth = 0;
    loop {
        let n = cursor.node();
        if n.kind() == "pair"
            && let Some(key) = n.child_by_field_name("key")
        {
            let key_text = key.utf8_text(source).unwrap_or("");
            if (key_text == "noent" || key_text == "externalEntities")
                && let Some(val) = n.child_by_field_name("value")
                && val.utf8_text(source).unwrap_or("") == "false"
            {
                return true;
            }
        }
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if depth == 0 {
                return false;
            }
            cursor.goto_parent();
            depth -= 1;
        }
    }
}

crate::ast_check! { on ["new_expression", "call_expression"] prefilter = ["DOMParser", "XMLParser", "parseXml", "xml2js"] => |node, source, ctx, diagnostics|
    // Match `new DOMParser()`, `new XMLParser()`, `new XMLParser({...})`
    // and `require('xml2js')`, `parseXml(...)`.
    let (is_xml_parser, check_node) = match node.kind() {
        "new_expression" => {
            let Some(constructor) = node.child_by_field_name("constructor") else { return };
            let name = constructor.utf8_text(source).unwrap_or("");
            (XML_PARSER_NAMES.contains(&name), node)
        }
        "call_expression" => {
            let Some(callee) = node.child_by_field_name("function") else { return };
            let name = match callee.kind() {
                "identifier" => callee.utf8_text(source).unwrap_or(""),
                _ => return,
            };
            if name == "require" {
                // Check if argument is 'xml2js'.
                let Some(args) = node.child_by_field_name("arguments") else { return };
                let Some(first) = args.named_child(0) else { return };
                if first.kind() != "string" { return; }
                let inner = {
                    let text = first.utf8_text(source).unwrap_or("");
                    if text.len() >= 2 { &text[1..text.len() - 1] } else { text }
                };
                (XML_PARSER_MODULES.contains(&inner), node)
            } else if name == "parseXml" {
                (true, node)
            } else {
                return;
            }
        }
        _ => return,
    };

    if !is_xml_parser {
        return;
    }

    // Check arguments for XXE protection.
    if let Some(args) = node.child_by_field_name("arguments")
        && has_protection(args, source) {
            return;
        }

    let pos = check_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-xml-external-entity".into(),
        message: "XML parser without XXE protection — set `noent: false` or `externalEntities: false`.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
