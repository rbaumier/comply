//! no-keyword-prefix backend — flag identifiers starting with `new` or `class`
//! followed by an uppercase letter (camelCase convention).

use crate::diagnostic::{Diagnostic, Severity};

const DISALLOWED_PREFIXES: &[&str] = &["new", "class"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "identifier" {
        return;
    }

    // Only check at declaration sites to avoid duplicate reports.
    if !is_declaration_site(node) {
        return;
    }

    let name = match node.utf8_text(source) {
        Ok(n) => n,
        Err(_) => return,
    };

    let keyword = match find_keyword_prefix(name) {
        Some(k) => k,
        None => return,
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-keyword-prefix".into(),
        message: format!(
            "Do not prefix identifiers with keyword `{keyword}`."
        ),
        severity: Severity::Warning,
    });
}

/// Check if an identifier name starts with a disallowed keyword followed by
/// an uppercase letter (camelCase convention: `newUser`, `classNames`).
fn find_keyword_prefix(name: &str) -> Option<&'static str> {
    for &prefix in DISALLOWED_PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix)
            && rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                return Some(prefix);
            }
    }
    None
}

/// Returns `true` when this identifier node sits in a position that declares
/// or binds a name (variable, parameter, function name, class name, etc.),
/// as opposed to a reference/usage site.
fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let parent = match node.parent() {
        Some(p) => p,
        None => return false,
    };
    matches!(
        parent.kind(),
        "variable_declarator"
            | "function_declaration"
            | "function"
            | "class_declaration"
            | "class"
            | "formal_parameters"
            | "required_parameter"
            | "optional_parameter"
            | "rest_pattern"
            | "catch_clause"
            | "import_specifier"
            | "import_clause"
            | "namespace_import"
            | "shorthand_property_identifier_pattern"
            | "for_in_statement"
            | "arrow_function"
            | "method_definition"
            | "public_field_definition"
            | "property_signature"
            | "enum_declaration"
            | "type_alias_declaration"
            | "interface_declaration"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_new_prefix() {
        let d = run_on("const newUser = getUser();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`new`"));
    }

    #[test]
    fn flags_class_prefix() {
        let d = run_on("const classNames = getClasses();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`class`"));
    }

    #[test]
    fn allows_plain_new() {
        // `new` by itself is a keyword, not an identifier with a prefix
        assert!(run_on("const x = new Map();").is_empty());
    }

    #[test]
    fn allows_lowercase_after_prefix() {
        // `newborn` does not have an uppercase letter after `new`
        assert!(run_on("const newborn = true;").is_empty());
    }

    #[test]
    fn allows_classify() {
        // `classify` has `class` prefix but `i` is lowercase
        assert!(run_on("const classify = (x: number) => x;").is_empty());
    }

    #[test]
    fn flags_function_param() {
        let d = run_on("function f(newValue: string) {}");
        assert_eq!(d.len(), 1);
    }
}
