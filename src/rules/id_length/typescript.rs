//! id-length TS/TSX/JS backend.
//!
//! Only flags identifiers at *binding* positions: variable
//! declarations, function / class / method / interface names, and
//! (destructured) function parameters. Usages and references are left
//! alone — they're not where a name is introduced.

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "identifier",
            "property_identifier",
            "shorthand_property_identifier_pattern",
            "type_identifier",
        ])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let min = ctx.config.threshold("id-length", "min", ctx.lang);
        let exceptions = ctx.config.string_list("id-length", "exceptions", ctx.lang);
        let patterns = compile_patterns(&ctx.config.string_list("id-length", "exception_patterns", ctx.lang));

        let source_bytes = ctx.source.as_bytes();
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
        if is_conventional_short_binding(node, name) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "id-length".into(),
            message: format!("Identifier `{name}` is too short (< {min})."),
            severity: Severity::Error,
            span: None,
        });
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

/// Single-letter names conventional in JS/TS: loop indices, callback
/// params, event/error handlers, generic type names.
const CONVENTIONAL_TS_NAMES: &[&str] = &[
    "i", "j", "k", "n", "x", "y", "z", "e", "v", "s", "f", "a", "b",
    "c", "d", "r", "m", "p", "w", "h", "g",
];

/// Allow conventional single-letter names in parameters, for-loop vars,
/// and variable declarations. Also allow single uppercase letters
/// (generic type parameter naming convention).
fn is_conventional_short_binding(node: tree_sitter::Node, name: &str) -> bool {
    if name.len() == 1 && name.chars().next().unwrap().is_ascii_uppercase() {
        return true;
    }
    if !CONVENTIONAL_TS_NAMES.contains(&name) {
        return false;
    }
    // Destructuring: `const { x } = obj` — node itself is the binding
    if node.kind() == "shorthand_property_identifier_pattern" {
        return true;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "required_parameter"
            | "optional_parameter"
            | "variable_declarator"
    )
}

/// True if `parent.child_by_field_name(field)` is the same AST node as
/// `node` (same byte range + same parent → same node identity).
fn field_matches(parent: tree_sitter::Node, field: &str, node: tree_sitter::Node) -> bool {
    parent
        .child_by_field_name(field)
        .is_some_and(|f| f.byte_range() == node.byte_range())
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
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
    fn allows_conventional_const() {
        // `x` is a conventional single-letter name in a variable_declarator
        assert!(run_on("const x = 1;").is_empty());
    }

    #[test]
    fn flags_unconventional_const() {
        let diags = run_on("const q = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`q`"));
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
    fn allows_conventional_function_parameter() {
        // `x` is conventional in a parameter
        assert!(run_on("function fn(x: number) { return x; }").is_empty());
    }

    #[test]
    fn flags_unconventional_function_parameter() {
        let diags = run_on("function fn(q: number) { return q; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`q`"));
    }

    #[test]
    fn does_not_flag_usage_only_references() {
        // `foo(x)` references `x`; the declaration of `x` is elsewhere,
        // so we should NOT flag the call site.
        assert!(run_on("function myFunction() { foo(x); }").is_empty());
    }

    #[test]
    fn allows_conventional_destructuring_binding() {
        assert!(run_on("const { x } = someObj;").is_empty());
    }

    #[test]
    fn flags_unconventional_destructuring_binding() {
        let diags = run_on("const { q } = someObj;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`q`"));
    }

    #[test]
    fn default_exceptions_allow_t_destructuring() {
        // The whole point of the `exceptions = ["t", "T"]` default:
        // `const { t } = useTranslation()` must stay clean.
        assert!(run_on("const { t } = useTranslation();").is_empty());
    }

    #[test]
    fn allows_single_uppercase_class_name() {
        // Single uppercase letter = conventional generic-style naming
        assert!(run_on("class X {}").is_empty());
    }

    #[test]
    fn allows_single_uppercase_interface_name() {
        assert!(run_on("interface U {}").is_empty());
    }

    #[test]
    fn allows_single_uppercase_type_alias() {
        assert!(run_on("type U = number;").is_empty());
    }

    #[test]
    fn tsx_allows_conventional_component_names() {
        // `D` single uppercase = allowed, `x` conventional = allowed
        assert!(run_tsx("const D = ({ x }: { x: string }) => <div>{x}</div>;").is_empty());
    }

    #[test]
    fn allows_conventional_callback_arrow_params() {
        assert!(run_on("arr.map((x) => x + 1);").is_empty());
        assert!(run_on("arr.forEach((v, i) => console.log(v, i));").is_empty());
    }

    #[test]
    fn allows_conventional_for_loop_variable() {
        assert!(run_on("for (let i = 0; i < 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_underscore_discard() {
        assert!(run_on("const _ = unused();").is_empty());
    }

    #[test]
    fn message_names_the_identifier() {
        let diags = run_on("const abc = 1;\nconst q = 2;");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "Identifier `q` is too short (< 2).");
    }
}
