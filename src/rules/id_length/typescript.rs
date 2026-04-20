//! id-length TS/TSX/JS backend.
//!
//! Only flags identifiers at *binding* positions: variable
//! declarations, function / class / method / interface names, and
//! (destructured) function parameters. Usages and references are left
//! alone — they're not where a name is introduced.

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let min = ctx.config.threshold("id-length", "min", 2);
        let exceptions = ctx.config.string_list("id-length", "exceptions");
        let patterns = compile_patterns(&ctx.config.string_list("id-length", "exception_patterns"));

        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if !is_binding_name(node) {
                return;
            }
            let Ok(name) = node.utf8_text(source_bytes) else {
                return;
            };
            if name.chars().count() >= min {
                return;
            }
            if exceptions.iter().any(|e| e == name) {
                return;
            }
            if patterns.iter().any(|p| p.is_match(name)) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "id-length".into(),
                message: format!("Identifier `{name}` is too short (< {min})."),
                severity: Severity::Error,
                span: None,
            });
        });

        diagnostics
    }
}

/// True when `node` is an identifier that introduces a new binding the
/// user has control over — where renaming is a one-shot local refactor
/// rather than a breaking change of an external symbol.
fn is_binding_name(node: tree_sitter::Node) -> bool {
    let kind = node.kind();
    if kind != "identifier"
        && kind != "property_identifier"
        && kind != "shorthand_property_identifier_pattern"
        && kind != "type_identifier"
    {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    // Destructuring shorthand: `const { t } = …` or `({ t }: Props) => …`.
    // The pattern node itself IS the binding.
    if kind == "shorthand_property_identifier_pattern" {
        return true;
    }
    match parent.kind() {
        // `const/let/var <name> = …` — the declarator's `name` field.
        "variable_declarator" => field_matches(parent, "name", node),
        // `function <name>() {}` / `class <name> {}` / `enum <name>` /
        // `interface <name>` / `type <name> = …`.
        "function_declaration"
        | "class_declaration"
        | "method_definition"
        | "enum_declaration"
        | "interface_declaration"
        | "type_alias_declaration" => field_matches(parent, "name", node),
        // Function parameters: `function f(<pattern>) {}` where the
        // pattern is a bare identifier.
        "required_parameter" | "optional_parameter" => field_matches(parent, "pattern", node),
        // Arrow / function expressions sometimes expose the parameter
        // directly as a child without a `required_parameter` wrapper.
        "arrow_function" | "function_expression" => false,
        _ => false,
    }
}

/// True if `parent.child_by_field_name(field)` is the same AST node as
/// `node` (same byte range + same parent → same node identity).
fn field_matches(parent: tree_sitter::Node, field: &str, node: tree_sitter::Node) -> bool {
    parent
        .child_by_field_name(field)
        .is_some_and(|f| f.byte_range() == node.byte_range())
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    fn run_tsx(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_short_const() {
        // Uses `x` rather than `t` — `t` is in the default exceptions
        // list (shipped for react-i18next's useTranslation()).
        let diags = run_on("const x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`x`"));
        assert!(diags[0].message.contains("< 2"));
    }

    #[test]
    fn allows_long_const() {
        assert!(run_on("const foo = 1;").is_empty());
    }

    #[test]
    fn default_exceptions_allow_t_binding() {
        // `t` is in `exceptions` in defaults.toml — must not be flagged.
        assert!(run_on("const t = 1;").is_empty());
    }

    #[test]
    fn flags_short_function_parameter() {
        let diags = run_on("function fn(x: number) { return x; }");
        // `fn` is 2 chars → passes (>=2). Only `x` fails.
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`x`"));
    }

    #[test]
    fn does_not_flag_usage_only_references() {
        // `foo(x)` references `x`; the declaration of `x` is elsewhere,
        // so we should NOT flag the call site.
        assert!(run_on("function myFunction() { foo(x); }").is_empty());
    }

    #[test]
    fn flags_short_destructuring_binding() {
        // `x` rather than `t` — `t` is exempt by default.
        let diags = run_on("const { x } = someObj;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`x`"));
    }

    #[test]
    fn default_exceptions_allow_t_destructuring() {
        // The whole point of the `exceptions = ["t", "T"]` default:
        // `const { t } = useTranslation()` must stay clean.
        assert!(run_on("const { t } = useTranslation();").is_empty());
    }

    #[test]
    fn flags_short_class_name() {
        let diags = run_on("class X {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`X`"));
    }

    #[test]
    fn flags_short_interface_name() {
        // `U` rather than `T` — `T` is in defaults exceptions.
        let diags = run_on("interface U {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`U`"));
    }

    #[test]
    fn flags_short_type_alias() {
        let diags = run_on("type U = number;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`U`"));
    }

    #[test]
    fn tsx_flags_short_component_prop_destructuring() {
        // Use `D` as component name (not `C` which passes at 1-char
        // min but here min=2). `x` as destructured prop (not `t`).
        let diags = run_tsx("const D = ({ x }: { x: string }) => <div>{x}</div>;");
        let names: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(names.iter().any(|m| m.contains("`D`")));
        assert!(names.iter().any(|m| m.contains("`x`")));
    }

    #[test]
    fn message_names_the_identifier() {
        let diags = run_on("const abc = 1;\nconst x = 2;");
        // `abc` is 3 chars, passes min 2. Only `x` fails.
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "Identifier `x` is too short (< 2)."
        );
    }
}
