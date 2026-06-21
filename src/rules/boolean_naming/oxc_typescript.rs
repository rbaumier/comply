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

/// Standard Web Audio / media-element / audio-mixer boolean property names. A
/// `set name(name: boolean)` accessor that mirrors one of these is bound to the
/// platform's exact spelling — `loop` mirrors `AudioBufferSourceNode.loop` /
/// `HTMLMediaElement.loop`, `muted`/`mute` mirror `HTMLMediaElement.muted` and
/// mixer terminology, `solo` is standard sequencer/DAW terminology. Renaming
/// the setter to `isLoop` would break the property contract. Used only in the
/// setter-accessor context (see `is_setter_accessor_param`), never for plain
/// variables or non-accessor parameters.
const MEDIA_API_BOOLEAN_PROPS: &[&str] = &["loop", "mute", "muted", "solo"];

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

/// A SCREAMING_SNAKE_CASE / ALL-CAPS identifier is a named-constant value label
/// (e.g. `LTR`, `RTL`, `OPERATOR`, `OPERAND`), not a predicate variable. The
/// name encodes a value from a domain vocabulary — often a 2-variant enum
/// expressed as a boolean — so a predicate prefix (`isLTR`) would be nonsensical.
fn is_screaming_case(name: &str) -> bool {
    let mut has_letter = false;
    for c in name.chars() {
        if c.is_ascii_lowercase() {
            return false;
        }
        if c.is_ascii_uppercase() {
            has_letter = true;
        }
    }
    has_letter
}

/// Return a short problem description if the name doesn't match the rule.
fn classify_name(name: &str) -> Option<&'static str> {
    // A leading `$` sigil marks a spec/framework-bound name (jQuery, Vue
    // `$`-props, JSON Schema `$`-keywords like `$async`/`$data`), not a
    // developer-chosen camelCase boolean. Such names mirror an external
    // contract and the `$is*` form is not idiomatic, so the predicate-prefix
    // convention does not apply.
    if name.starts_with('$') {
        return None;
    }
    if is_screaming_case(name) {
        return None;
    }
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

/// True when `node` is the parameter of a `set name(...)` accessor whose
/// property name equals `param_name`. Walks up to the parameter's owning
/// function (the setter's own `Function`); that function's enclosing element
/// must be a non-computed `set` `MethodDefinition` keyed by the same name. A
/// non-accessor parameter (constructor, method, free function) never matches,
/// so strictness is preserved outside this context.
fn is_setter_accessor_param(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    param_name: &str,
) -> bool {
    let mut saw_owning_function = false;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            // The setter's own function body. Any further function boundary
            // would mean the parameter belongs to a nested function, not the
            // accessor — bail out in that case.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                if saw_owning_function {
                    return false;
                }
                saw_owning_function = true;
            }
            AstKind::MethodDefinition(method) => {
                return method.kind == MethodDefinitionKind::Set
                    && !method.computed
                    && setter_key_matches(&method.key, param_name);
            }
            // An accessor declared as a class field (`accessor`-shaped) — not a
            // setter parameter context.
            AstKind::Class(_) => return false,
            _ => {}
        }
    }
    false
}

/// True if the accessor key is a plain identifier (or quoted-string literal)
/// equal to `param_name`. Computed keys are rejected by the `!method.computed`
/// guard in `is_setter_accessor_param` before this is reached.
fn setter_key_matches(key: &PropertyKey, param_name: &str) -> bool {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name == param_name,
        PropertyKey::StringLiteral(s) => s.value == param_name,
        _ => false,
    }
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
                // A `set loop(loop: boolean)` accessor mirroring a standard
                // Web Audio / media-element boolean property is bound to the
                // platform's exact name; a predicate prefix would break the
                // property contract. Both gates required: setter-accessor
                // context AND a recognized platform boolean property name.
                if MEDIA_API_BOOLEAN_PROPS.contains(&name)
                    && is_setter_accessor_param(node, semantic, name)
                {
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

    #[test]
    fn no_fp_on_dollar_prefixed_spec_boolean() {
        // `$`-sigil names mirror spec/framework contracts (JSON Schema
        // `$async`/`$data` keywords in ajv) and cannot take an `is*` prefix.
        // (Closes #5290)
        assert!(run("function f($async: boolean) {}").is_empty());
        assert!(run("function g($data?: boolean) {}").is_empty());
        assert!(run("const $async: boolean = true;").is_empty());
    }

    #[test]
    fn still_flags_unprefixed_boolean_alongside_dollar_exemption() {
        // Strictness preserved: only the leading `$` is exempt; ordinary
        // unprefixed booleans still require a predicate prefix.
        assert_eq!(run("const optional: boolean = true;").len(), 1);
        assert_eq!(run("let debug = false;").len(), 1);
    }

    #[test]
    fn no_fp_on_screaming_case_boolean_constant() {
        // ALL-CAPS boolean constants are named value labels encoding a 2-variant
        // enum (associativity direction, token kind), not predicate variables.
        // A predicate prefix (`isLTR`) would be nonsensical. (Closes #5069)
        assert!(run("const LTR = true;").is_empty());
        assert!(run("const RTL = false;").is_empty());
        assert!(run("const OPERATOR = true;").is_empty());
        assert!(run("const OPERAND = false;").is_empty());
        assert!(run("const LEFT_TO_RIGHT = true;").is_empty());
    }

    #[test]
    fn still_flags_lowercase_adjective_boolean() {
        // Strictness preserved: regular (non-ALL-CAPS) adjective/state boolean
        // variables still require a predicate prefix.
        assert_eq!(run("let ready = true;").len(), 1);
        assert_eq!(run("let active = false;").len(), 1);
        // Lowercase counterparts of the exempt constant names still flag —
        // only the ALL-CAPS form is a value label.
        assert_eq!(run("let operator = true;").len(), 1);
    }

    #[test]
    fn no_fp_on_media_api_setter_accessor_param() {
        // A `set name(name: boolean)` accessor mirroring a standard Web Audio /
        // media-element boolean property is bound to the platform's exact name;
        // a predicate prefix would break the property contract. (Closes #5074)
        assert!(
            run("class S { set loop(loop: boolean) { this._loop = loop; } }").is_empty()
        );
        assert!(
            run("class S { set mute(mute: boolean) { this._mute = mute; } }").is_empty()
        );
        assert!(
            run("class S { set muted(muted: boolean) { this._muted = muted; } }").is_empty()
        );
        assert!(
            run("class S { set solo(solo: boolean) { this._solo = solo; } }").is_empty()
        );
    }

    #[test]
    fn media_api_exemption_requires_setter_accessor_context() {
        // Both gates required. A recognized media-API name as a plain variable,
        // a non-accessor function parameter, or a constructor parameter still
        // flags — the exemption is scoped to the setter-accessor context only.
        assert_eq!(run("let mute = false;").len(), 1);
        assert_eq!(run("const loop: boolean = true;").len(), 1);
        assert_eq!(run("function f(mute: boolean) {}").len(), 1);
        assert_eq!(
            run("class S { constructor(solo?: boolean) {} }").len(),
            1
        );
        // A getter is not a value-bearing setter context.
        assert_eq!(
            run("class S { method(loop: boolean) {} }").len(),
            1
        );
    }

    #[test]
    fn setter_accessor_exemption_requires_recognized_media_api_name() {
        // Both gates required. An unrecognized adjective name in a setter
        // accessor still flags — only genuine platform boolean properties are
        // exempt, so ordinary boolean setters keep needing a prefix.
        assert_eq!(
            run("class S { set debug(debug: boolean) { this._debug = debug; } }").len(),
            1
        );
        assert_eq!(
            run("class S { set ready(ready: boolean) { this._ready = ready; } }").len(),
            1
        );
    }

    #[test]
    fn computed_key_setter_still_flags() {
        // A computed accessor key (`set ['loop'](...)`) is not the canonical
        // platform-property mirror; the name is dynamic, so strictness holds.
        assert_eq!(
            run("class S { set ['loop'](loop: boolean) { this._loop = loop; } }").len(),
            1
        );
    }
}
