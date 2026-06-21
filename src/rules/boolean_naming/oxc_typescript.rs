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
/// are dictated by the platform / component library API, plus ECMA-402 Intl
/// option keys (`hour12`) the developer cannot rename.
const ALLOWED_NAMES: &[&str] = &[
    "open", "checked", "disabled", "enabled", "hidden", "required", "selected",
    "readOnly", "multiple", "autoFocus", "autoPlay", "defer", "async",
    "noValidate", "value", "defaultOpen", "defaultChecked", "hour12",
];

/// True if the name ends in the explicit `flag` suffix as a distinct word
/// (`useDeltaFlag`, `use_delta_flag`, or bare `flag`). The `flag` suffix is
/// itself a boolean marker — as clear an intent signal as an `is*`/`has*`
/// prefix — and is the verbatim naming convention for boolean syntax elements
/// in ITU-T/ISO codec and protocol specifications. A trailing `flag` mid-word
/// (`flagged`) does not match: the word boundary (camelCase `Flag` or
/// snake_case `_flag`) is required, so adjective/state names still need a
/// prefix.
fn has_flag_suffix(name: &str) -> bool {
    name == "flag" || name.ends_with("Flag") || name.ends_with("_flag")
}

/// Return a short problem description if the name doesn't match the rule.
fn classify_name(name: &str) -> Option<&'static str> {
    if NEGATIVE_SUBSTRINGS.iter().any(|neg| name.contains(neg)) {
        return Some("is negatively phrased — use the positive form with `!`");
    }
    if has_flag_suffix(name) {
        return None;
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    fn run_in_test_file(s: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, s, "t.tsx", crate::project::default_static_project_ctx(), &file)
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
        assert_eq!(run("const retry: boolean = true;").len(), 1);
    }

    #[test]
    fn no_fp_on_flag_suffix() {
        // The explicit `flag` suffix is itself a boolean marker — the verbatim
        // naming convention for boolean syntax elements in ITU-T/ISO codec
        // specs (HEVC/H.265, H.264). Renaming would break correspondence with
        // the standard. (Closes #5065)
        assert!(run("let inter_ref_pic_set_prediction_flag = false;").is_empty());
        assert!(run("let use_delta_flag: boolean = false;").is_empty());
        assert!(run("let sps_temporal_id_nesting_flag: boolean = true;").is_empty());
        assert!(run("let defaultDisplayWindowFlag = false;").is_empty());
    }

    #[test]
    fn flag_suffix_does_not_soften_adjective_strictness() {
        // The `flag` suffix only validates a trailing-word `flag`; bare
        // adjective/state names without a prefix or flag suffix still flag.
        assert_eq!(run("const debug: boolean = true;").len(), 1);
        assert_eq!(run("let ready = false;").len(), 1);
        // A mid-word `flag` (e.g. `flagged`) is not the boolean-marker suffix.
        assert_eq!(run("let flagged: boolean = true;").len(), 1);
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

    #[test]
    fn no_fp_on_api_mandated_hour12() {
        // `hour12` is the ECMA-402 Intl.DateTimeFormat option key; the developer
        // cannot rename it to `isHour12`. (Closes #4997)
        assert!(run("const hour12: boolean = true;").is_empty());
        assert!(run("function f(hour12: boolean) {}").is_empty());
    }

    #[test]
    fn still_flags_user_defined_unprefixed_boolean() {
        // Strictness preserved: user-controlled names still require a prefix.
        assert_eq!(run("const debug: boolean = true;").len(), 1);
    }
}
