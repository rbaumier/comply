//! boolean-naming OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

const VALID_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "was", "in", "seen", "found",
];
const NEGATIVE_SUBSTRINGS: &[&str] = &["Not", "Isnt", "Cannot", "Cant", "Shouldnt"];

/// Standard HTML attributes and React controlled-component props whose names
/// are dictated by the platform / component library API.
const ALLOWED_NAMES: &[&str] = &[
    "open", "checked", "disabled", "enabled", "hidden", "required", "selected",
    "readOnly", "multiple", "autoFocus", "autoPlay", "defer", "async",
    "noValidate", "value", "defaultOpen", "defaultChecked",
];

/// Return a short problem description if the name doesn't match the rule.
fn classify_name(name: &str) -> Option<&'static str> {
    if NEGATIVE_SUBSTRINGS.iter().any(|neg| name.contains(neg)) {
        return Some("is negatively phrased — use the positive form with `!`");
    }
    for &prefix in VALID_PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix)
            && (rest.is_empty() || rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())) {
                return None;
            }
    }
    Some("is missing a predicate prefix")
}

/// Check if a type annotation is `: boolean`.
fn is_boolean_annotation(annotation: &TSTypeAnnotation) -> bool {
    matches!(&annotation.type_annotation, TSType::TSBooleanKeyword(_))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Ambient bindings inside `declare global` / `declare module` are
        // type-level only — there is no runtime variable to name.
        if crate::oxc_helpers::is_in_ambient_declaration(node.id(), semantic) {
            return;
        }

        // Test files use idiomatic boolean state flags (`initialized`,
        // `serveRenamed`) that don't benefit from the prefix rule.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        let (name, span, is_bool) = match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                let BindingPattern::BindingIdentifier(ref id) = decl.id else {
                    return;
                };
                let name = id.name.as_str();

                // Check for `: boolean` annotation
                let has_annotation = decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| is_boolean_annotation(ann));

                // Check for `= true` / `= false` initializer
                let has_bool_init = decl.init.as_ref().is_some_and(|init| {
                    matches!(init, Expression::BooleanLiteral(_))
                });

                if !has_annotation && !has_bool_init {
                    return;
                }
                (name, id.span, true)
            }
            AstKind::FormalParameter(param) => {
                let BindingPattern::BindingIdentifier(ref id) = param.pattern else {
                    return;
                };
                let name = id.name.as_str();
                let has_annotation = param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| is_boolean_annotation(ann));
                if !has_annotation {
                    return;
                }
                (name, id.span, true)
            }
            _ => return,
        };

        if !is_bool {
            return;
        }

        if ALLOWED_NAMES.contains(&name) {
            return;
        }

        let Some(problem) = classify_name(name) else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Boolean '{name}' {problem}. Use a predicate prefix: \
                 `is*`, `has*`, `should*`, `can*`, `will*`, `did*`, `was*`, \
                 `in*`, `seen*`, `found*`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }

    fn run_in_test_file(s: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_oxc_tsx_with_file_ctx(s, &Check, &file)
    }

    #[test]
    fn no_fp_on_boolean_var_in_declare_global() {
        // Ambient binding inside `declare global` is type-level only — there
        // is no runtime variable to name with a predicate prefix. (Closes #339)
        assert!(
            run("declare global {\n  var BASE_UI_ANIMATIONS_DISABLED: boolean;\n}\nexport {};")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_boolean_var_at_runtime() {
        assert_eq!(run("const enabledFlag: boolean = true;").len(), 1);
    }

    #[test]
    fn no_fp_on_test_idiomatic_boolean_state_var() {
        // Test files use short boolean flags to control test state and mock
        // behavior. (Closes #525)
        assert!(run_in_test_file("let initialized = false;").is_empty());
        assert!(run_in_test_file("let serveRenamed = false;").is_empty());
    }

    #[test]
    fn still_flags_in_non_test_file() {
        assert_eq!(run("let initialized = false;").len(), 1);
        assert_eq!(run("let serveRenamed = false;").len(), 1);
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn allows_is_prefix() {
        assert!(run_on("const isReady: boolean = true;").is_empty());
    }


    #[test]
    fn allows_has_prefix() {
        assert!(run_on("const hasItems: boolean = false;").is_empty());
    }


    #[test]
    fn allows_should_will_did_was() {
        for name in ["shouldRetry", "willSucceed", "didFire", "wasLoaded"] {
            let source = format!("const {name} = true;");
            assert!(run_on(&source).is_empty(), "'{name}' should be allowed");
        }
    }


    #[test]
    fn flags_missing_prefix_with_annotation() {
        let diags = run_on("const ready: boolean = true;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'ready'"));
    }


    #[test]
    fn flags_inferred_boolean() {
        let diags = run_on("const ready = true;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_negative_phrasing() {
        let diags = run_on("const isNotReady = false;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("negatively"));
    }


    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // `issuer` starts with `is` but is not a boolean predicate.
        // It won't be flagged because its type isn't boolean.
        assert!(run_on("const issuer: string = 'ACME';").is_empty());
    }


    #[test]
    fn flags_param_without_prefix() {
        let diags = run_on("function f(ready: boolean) {}");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_controlled_component_props() {
        for name in ["open", "checked", "disabled", "hidden", "selected", "value"] {
            let source = format!("function F({name}: boolean) {{}}");
            assert!(run_on(&source).is_empty(), "'{name}' should be allowed");
        }
    }
}
