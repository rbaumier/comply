//! boolean-naming OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

// `are` is the plural copula: a valid predicate prefix for collective/plural
// booleans (`areMutuallyExclusive`, `areEqual`) that read as "the subjects are
// X", exactly like the singular `is`. `needs` is a necessity/modal verb that
// forms a predicate just like the allowed `should`/`will`: `needsBarrier`,
// `needsRefresh` read as "does it need X?". The camelCase-boundary guard in
// `classify_name` (the prefix must be followed by an uppercase letter) keeps
// noun names whose first letters happen to be `are`/`needs` — `area`, `arena` —
// out: they strip to a lowercase remainder and still flag.
const VALID_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "was", "in", "seen", "found", "are", "needs",
];
const NEGATIVE_SUBSTRINGS: &[&str] = &["Not", "Isnt", "Cannot", "Cant", "Shouldnt"];

/// Predicate verbs that, when appearing as a capitalized mid-name word
/// (`<subject>Is<Predicate>`, `<subject>Has<Noun>`, …), embed a predicate
/// relationship just as a leading prefix does. `nextIsSingle`,
/// `prevVNodeIsTextNode`, `valueHasOwner` read as "the subject is/has X" — the
/// infix `Is` serves the exact semantic function of an `is*` prefix while making
/// the subject explicit, so a redundant leading `is*` would be wrong. `Are` is
/// the plural copula: `parentFieldsAreMutuallyExclusive` reads "the parent
/// fields are mutually exclusive", the plural-subject counterpart of `Is`. This
/// is the camelCase counterpart of `INFIX_PREDICATES` in the rule's Rust backend
/// (`<noun>_is_<adjective>`); the verb set is kept in sync.
const INFIX_PREDICATES: &[&str] = &["Is", "Are", "Has", "Should", "Can", "Will"];

/// Standard HTML attributes and React controlled-component props whose names
/// are dictated by the platform / component library API, plus ECMA-402 Intl
/// option keys (`hour12`) the developer cannot rename.
const ALLOWED_NAMES: &[&str] = &[
    "open", "checked", "disabled", "enabled", "hidden", "required", "selected",
    "readOnly", "multiple", "autoFocus", "autoPlay", "defer", "async",
    "noValidate", "value", "defaultOpen", "defaultChecked", "hour12",
];

/// The three boolean-valued attributes of an ECMAScript property descriptor
/// (ECMA-262 §6.2.6: `enumerable`, `writable`, `configurable`). A wrapper around
/// `Object.defineProperty` must forward these under their exact spelling, and a
/// shorthand `{ enumerable }` forces the forwarding identifier to equal the key,
/// so the parameter's name is structurally fixed. Used only together with the
/// shorthand-into-`defineProperty` use-site (see `is_define_property_descriptor_param`),
/// never as a standalone name allowlist.
const DESCRIPTOR_BOOLEAN_KEYS: &[&str] = &["enumerable", "writable", "configurable"];

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

/// True if `name` embeds a predicate verb as a capitalized mid-name word —
/// `<subject>Is<Descriptor>` (`nextIsSingle`, `prevVNodeIsTextNode`),
/// `<subject>Has<Noun>` (`valueHasOwner`), etc. The verb must be a real infix
/// word: there is a non-empty subject before it (so a name beginning with the
/// verb itself, `IsReady`, is not treated as an infix) and the descriptor word
/// starts with an uppercase letter right after it. The trailing boundary
/// distinguishes the verb word from a longer word with the same opening letters
/// (`Issue` after a subject is not `Is`, `nextIssue` does not match), and a
/// trailing verb (`singleIs`) has no descriptor after it, so it is not an infix.
/// Mirrors `has_infix_predicate` in the rule's Rust backend.
fn has_infix_predicate(name: &str) -> bool {
    let bytes = name.as_bytes();
    INFIX_PREDICATES.iter().any(|verb| {
        let mut search_from = 0;
        while let Some(rel) = name[search_from..].find(verb) {
            let start = search_from + rel;
            let after = start + verb.len();
            let has_subject_before = start > 0;
            let descriptor_starts_word =
                bytes.get(after).is_some_and(|c| c.is_ascii_uppercase());
            if has_subject_before && descriptor_starts_word {
                return true;
            }
            search_from = start + 1;
        }
        false
    })
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

/// True when `name` begins with a boolean-predicate prefix (`is`/`has`/`should`/
/// `can`/…) at a camelCase word boundary: the prefix is either the whole name or
/// is immediately followed by an uppercase letter. `isServer` matches; `island`
/// does not (`is` is followed by the lowercase `l`), so noun names sharing a
/// prefix's opening letters still lack a predicate.
///
/// This is the single definition of the predicate-prefix convention, consumed
/// both here and by `screaming-snake-for-constants`: a boolean-literal constant
/// whose name follows this convention would violate `boolean-naming` if renamed
/// to SCREAMING_SNAKE_CASE, so that rule exempts it.
pub(crate) fn has_boolean_predicate_prefix(name: &str) -> bool {
    VALID_PREFIXES.iter().any(|&prefix| {
        name.strip_prefix(prefix).is_some_and(|rest| {
            rest.is_empty() || rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        })
    })
}

/// True if `name` contains a negation word (`Not`, `Isnt`, `Cannot`, `Cant`,
/// `Shouldnt`) at a camelCase word boundary. Each entry begins with an uppercase
/// letter, so it starts a camelCase word; it must also *end* one — the character
/// right after the match is absent (end of name) or another uppercase letter that
/// opens the next word. `valueIsNotSet` (the `S` of `Set` follows `Not`) is a
/// negation; `abortNotified` (`Not` followed by lowercase `i`, the past
/// participle of "notify") is not. This is the camelCase counterpart of the
/// `_not_` snake_case word boundary in the rule's Rust backend.
fn contains_negative_word(name: &str) -> bool {
    let bytes = name.as_bytes();
    NEGATIVE_SUBSTRINGS.iter().any(|neg| {
        let mut search_from = 0;
        while let Some(rel) = name[search_from..].find(neg) {
            let start = search_from + rel;
            let after = start + neg.len();
            let ends_word = bytes.get(after).is_none_or(|c| c.is_ascii_uppercase());
            if ends_word {
                return true;
            }
            search_from = start + 1;
        }
        false
    })
}

/// Return a short problem description if the name doesn't match the rule.
fn classify_name(name: &str) -> Option<&'static str> {
    // A leading underscore is a convention marker (private/internal state,
    // intentionally-unused binding), not part of the predicate. Strip it so
    // `_hasWarnedZodMismatch` is classified on `hasWarnedZodMismatch`: a valid
    // `has*` prefix is no longer hidden by the sigil. Strictness is preserved —
    // `_ready` strips to `ready`, which still lacks a prefix and flags.
    let name = name.trim_start_matches('_');

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
    if contains_negative_word(name) {
        return Some("is negatively phrased — use the positive form with `!`");
    }
    if has_flag_suffix(name) {
        return None;
    }
    if has_boolean_predicate_prefix(name) {
        return None;
    }
    // A `<subject>Is<Descriptor>` compound (`nextIsSingle`) embeds the predicate
    // as a capitalized infix word, the same intent signal as a leading `is*`
    // prefix, so a redundant prefix would be wrong.
    if has_infix_predicate(name) {
        return None;
    }
    Some("is missing a predicate prefix")
}

/// Check if a type annotation is `: boolean`.
fn is_boolean_annotation(annotation: &TSTypeAnnotation) -> bool {
    matches!(&annotation.type_annotation, TSType::TSBooleanKeyword(_))
}

/// True when a union member is boolean-ish: the `boolean` keyword, a `true` /
/// `false` literal, or the nullish keywords `undefined` / `null`. A union built
/// only from these reads as a boolean (`boolean | undefined` is still a
/// predicate), so it keeps requiring a predicate prefix. Any other member — a
/// `string`, a `number`, a type reference (`AstObject`), a string literal — makes
/// the union a value-or-sentinel type rather than a boolean.
fn is_boolean_ish_member(ty: &TSType) -> bool {
    match ty {
        TSType::TSBooleanKeyword(_)
        | TSType::TSUndefinedKeyword(_)
        | TSType::TSNullKeyword(_) => true,
        TSType::TSLiteralType(lit) => matches!(&lit.literal, TSLiteral::BooleanLiteral(_)),
        _ => false,
    }
}

/// True when a type annotation is a `TSUnionType` carrying a non-boolean member
/// (`string | false`, `AstObject | false`, `number | false`). Such a variable
/// holds string/object/number content OR a `false` sentinel — it is an
/// optional-value pattern, not a boolean predicate, so the `is*` prefix
/// convention does not apply. A pure-boolean union (`boolean | undefined`) is not
/// matched and still flags.
fn is_non_boolean_union_annotation(annotation: &TSTypeAnnotation) -> bool {
    match &annotation.type_annotation {
        TSType::TSUnionType(union) => union.types.iter().any(|m| !is_boolean_ish_member(m)),
        _ => false,
    }
}

/// True when a `boolean` parameter is an opt-in configuration flag: it is
/// optional (`colored?: boolean`) or carries a default value
/// (`partial: boolean = false`). An omittable boolean parameter is structurally
/// a toggle the caller may flip — the published-API flag shape used pervasively
/// in CLI / formatter / parser APIs (`colored`, `partial`, `verbose`) — where an
/// adjective reads as the mode it selects, not as a predicate on some noun.
/// Renaming such a flag to `isColored` would diverge from the framework's
/// established public vocabulary and break backward compatibility.
///
/// Anchored on the parameter's own structure (`optional` marker or a default
/// initializer), so it cannot widen into a name allowlist: a *required* boolean
/// parameter (`colored: boolean`) is not an opt-in flag and still requires a
/// predicate prefix, and plain boolean variables are unaffected.
fn is_optional_flag_param(param: &FormalParameter) -> bool {
    param.optional || param.initializer.is_some()
}

/// True when the parameter belongs to a type-only callable signature rather than
/// a runtime function — an interface/type-literal method signature, a function or
/// constructor type (`type F = (flag: boolean) => boolean`), a call/construct
/// signature, or a body-less `Function` (an abstract method or a function
/// overload signature). In every such position the parameter declares no runtime
/// binding: its identifier is a pure type-level contract, not a variable that
/// could carry a predicate prefix. This mirrors the rule's existing
/// ambient-declaration exemption (type-level only — no runtime variable to name)
/// and the `is_runtime_function_param` gate of `no-boolean-flag-param`.
///
/// Anchored on the enclosing-callable shape, not a name list: the same parameter
/// inside a concrete (body-bearing) `Function`/arrow is a real runtime binding
/// and still requires a predicate prefix.
fn is_type_only_signature_param(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let params_node = nodes.parent_node(node.id());
    if !matches!(params_node.kind(), AstKind::FormalParameters(_)) {
        return false;
    }
    match nodes.parent_node(params_node.id()).kind() {
        // A concrete, body-bearing function/arrow is the only runtime binding
        // site; a body-less `Function` is an abstract method or overload
        // signature, which declares no runtime variable.
        AstKind::Function(func) => func.body.is_none(),
        AstKind::ArrowFunctionExpression(_) => false,
        // Any other callable parent is a pure type-level signature.
        _ => true,
    }
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

/// True when the call is `Object.defineProperty(...)` / `Reflect.defineProperty(...)`,
/// whose third argument is the property descriptor object.
fn is_define_property_call(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "defineProperty"
        && matches!(
            &member.object,
            Expression::Identifier(obj) if matches!(obj.name.as_str(), "Object" | "Reflect")
        )
}

/// True when the boolean parameter `param_name` is forwarded — as a *shorthand*
/// property whose key is a boolean property-descriptor attribute (`enumerable`,
/// `writable`, `configurable`) — into the descriptor (third) argument of an
/// `Object.defineProperty` / `Reflect.defineProperty` call. A shorthand property
/// requires `identifier == key`, so the fixed ECMAScript descriptor key
/// structurally forces the parameter's exact name; renaming it to `isEnumerable`
/// would break both the shorthand and the descriptor contract.
///
/// Anchored on the use-site shape, not a name list: the same descriptor-key name
/// as a plain variable, a non-shorthand forwarding (`{ enumerable: flag }`), or a
/// shorthand inside any object literal that is not a `defineProperty` descriptor
/// argument, all still require a predicate prefix.
fn is_define_property_descriptor_param(
    symbol_id: Option<oxc_semantic::SymbolId>,
    param_name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_span::GetSpan;

    if !DESCRIPTOR_BOOLEAN_KEYS.contains(&param_name) {
        return false;
    }
    let Some(symbol_id) = symbol_id else {
        return false;
    };
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    scoping.get_resolved_references(symbol_id).any(|reference| {
        let ref_node = reference.node_id();
        // The reference is the value of a shorthand object property
        // (`{ enumerable }` — the identifier equals the key).
        let AstKind::ObjectProperty(prop) = nodes.kind(nodes.parent_id(ref_node)) else {
            return false;
        };
        if !prop.shorthand {
            return false;
        }
        // Grandparent is the descriptor object literal; its parent must be the
        // `defineProperty` call carrying it as the descriptor (third) argument.
        let object_id = nodes.parent_id(nodes.parent_id(ref_node));
        let AstKind::ObjectExpression(descriptor) = nodes.kind(object_id) else {
            return false;
        };
        let AstKind::CallExpression(call) = nodes.kind(nodes.parent_id(object_id)) else {
            return false;
        };
        is_define_property_call(call)
            && call.arguments.get(2).is_some_and(|arg| arg.span() == descriptor.span)
    })
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

                // A `string | false` / `AstObject | false` union annotation marks
                // a value-or-sentinel variable (it holds content OR the `false`
                // sentinel), not a boolean predicate; the `= false` initializer is
                // the sentinel, not a boolean. A pure-boolean union
                // (`boolean | undefined`) is not matched and still flags.
                if decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| is_non_boolean_union_annotation(ann))
                {
                    return;
                }

                // An explicit annotation whose top-level type is neither the
                // `boolean` keyword nor a union (`Listener["https"]` indexed
                // access, a `HTTPSCert` type reference, a conditional type, …)
                // is an opaque non-boolean type and is the source of truth for
                // the variable's type. A `= false` / `= true` initializer is a
                // sentinel value, not evidence of a boolean predicate, so the
                // init-value heuristic must not fire. `boolean` and boolean-ish
                // unions (`boolean | undefined`) fall through and still flag.
                if let Some(ann) = &decl.type_annotation {
                    let top = &ann.type_annotation;
                    if !matches!(top, TSType::TSBooleanKeyword(_) | TSType::TSUnionType(_)) {
                        return;
                    }
                }

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
                // A parameter in a type-only callable signature (interface method
                // signature, function/constructor type, abstract method, function
                // overload) declares no runtime binding — its name is a pure
                // type-level contract, not a variable to name. Concrete
                // (body-bearing) functions still flag.
                if is_type_only_signature_param(node, semantic) {
                    return;
                }
                // An optional / default-valued boolean parameter is an opt-in
                // configuration flag (the published-API toggle shape), not a
                // predicate variable; required boolean parameters still flag.
                if is_optional_flag_param(param) {
                    return;
                }
                // A parameter property (`constructor(public trainable: boolean)`)
                // declares an instance field, not a local binding; its name is the
                // class's property name, governed by the implemented data-model /
                // API contract. Plain field/interface-member declarations are
                // already out of scope, so the shorthand form is too.
                if crate::oxc_helpers::is_parameter_property(param) {
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
                // A required boolean parameter forwarded as a shorthand property
                // into an `Object.defineProperty` / `Reflect.defineProperty`
                // descriptor is bound to the ECMAScript descriptor attribute's
                // exact name (`enumerable` / `writable` / `configurable`); the
                // shorthand forces identifier == key, so an `is*` prefix would
                // break the descriptor contract.
                if is_define_property_descriptor_param(id.symbol_id.get(), name, semantic) {
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
                 `in*`, `seen*`, `found*`, `are*`, `needs*`."
            ),
            severity: Severity::Error,
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
            run("class S { constructor(solo: boolean) {} }").len(),
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

    #[test]
    fn no_fp_on_optional_flag_parameter() {
        // An optional / default-valued boolean parameter is an opt-in
        // configuration flag (the published-API toggle shape used across CLI /
        // formatter / parser frameworks), not a predicate variable. (Closes #5452)
        assert!(run("function f(colored?: boolean) {}").is_empty());
        assert!(run("function f(partial: boolean = false) {}").is_empty());
        assert!(run("class C { format(colored?: boolean): ColorFormat {} }").is_empty());
        assert!(run("export function runMachineInternal(partial: boolean = false) {}").is_empty());
        // The exemption is scoped to the parameter position by structure, not by
        // name: any optional/defaulted boolean param is treated as an opt-in
        // flag, while the same name as a plain variable still flags (see
        // `still_flags_lowercase_adjective_boolean`).
        assert!(run("function f(ready?: boolean) {}").is_empty());
        assert!(run("function f(done: boolean = false) {}").is_empty());
    }

    #[test]
    fn still_flags_required_boolean_parameter() {
        // Strictness preserved: a required boolean parameter is not an opt-in
        // flag and still requires a predicate prefix.
        assert_eq!(run("function f(colored: boolean) {}").len(), 1);
        assert_eq!(run("function f(partial: boolean) {}").len(), 1);
        assert_eq!(run("class C { format(colored: boolean) {} }").len(), 1);
    }

    #[test]
    fn no_fp_on_leading_underscore_prefixed_boolean() {
        // A leading underscore marks a private/internal boolean state flag; the
        // predicate prefix sits past the sigil, so `_hasWarnedZodMismatch` is a
        // valid `has*` name and must not flag. (Closes #5154)
        assert!(run("let _hasWarnedZodMismatch = false;").is_empty());
        assert!(run("let _isReady: boolean = true;").is_empty());
        assert!(run("let __hasValue: boolean = false;").is_empty());
        assert!(run("function f(_isEnabled: boolean) {}").is_empty());
        // Underscores strip before the `$` early-return, so a `_$`-sigil spec
        // name stays exempt (locks in the strip-then-`$` check ordering).
        assert!(run("const _$async: boolean = true;").is_empty());
    }

    #[test]
    fn still_flags_underscore_prefixed_boolean_without_predicate() {
        // Strictness preserved: stripping the leading underscore does not relax
        // the prefix requirement — `_ready` strips to `ready`, still unprefixed.
        assert_eq!(run("let _ready: boolean = true;").len(), 1);
        assert_eq!(run("let __debug = false;").len(), 1);
    }

    #[test]
    fn no_fp_on_parameter_property() {
        // A constructor parameter property declares an instance field; the
        // identifier is the class's property name, dictated by the implemented
        // data-model / API contract (`public trainable: boolean` realizes the
        // ML `Variable.trainable` field), not a free local predicate. Plain
        // class-field declarations are already out of scope, so the parameter-
        // property shorthand is exempt for the same reason. (Closes #5425)
        assert!(run("class V { constructor(public trainable: boolean) {} }").is_empty());
        assert!(run("class V { constructor(private verbose: boolean) {} }").is_empty());
        assert!(run("class V { constructor(protected keepDims: boolean) {} }").is_empty());
        assert!(run("class V { constructor(readonly alignCorners: boolean) {} }").is_empty());
        assert!(
            run("class V { constructor(public readonly trainable: boolean) {} }").is_empty()
        );
    }

    #[test]
    fn still_flags_plain_required_constructor_parameter() {
        // Strictness preserved: a constructor parameter without an accessibility
        // or `readonly` modifier is an ordinary local binding, not a field
        // declaration, so it still requires a predicate prefix.
        assert_eq!(run("class V { constructor(trainable: boolean) {} }").len(), 1);
        assert_eq!(run("class V { constructor(verbose: boolean) {} }").len(), 1);
    }

    #[test]
    fn no_fp_on_noun_is_adjective_infix_predicate() {
        // A capitalized predicate verb (`Is`/`Has`/…) appearing as a mid-name
        // word forms a `<subject>Is<Predicate>` proposition that reads as "the
        // subject is X", serving the same function as a leading `is*` prefix
        // while making the subject explicit (`next` vs `prev`). (Closes #5497)
        assert!(
            run("const nextIsSingle: boolean =\n  (nextChildFlags & 1) !== 0;").is_empty()
        );
        assert!(run("const prevVNodeIsTextNode: boolean = true;").is_empty());
        assert!(run("function f(valueHasOwner: boolean) {}").is_empty());
        assert!(run("function f(userCanEdit: boolean) {}").is_empty());
        assert!(run("function f(requestShouldRetry: boolean) {}").is_empty());
        assert!(run("function f(taskWillRun: boolean) {}").is_empty());
    }

    #[test]
    fn infix_predicate_requires_subject_before_and_descriptor_after() {
        // A trailing predicate verb (`singleIs`) has no descriptor after it, so
        // it still flags. A name beginning with the verb itself (`IsReady`) has
        // no subject before it, so it is not an infix and still flags.
        assert_eq!(run("let singleIs: boolean = true;").len(), 1);
        assert_eq!(run("let IsReady: boolean = true;").len(), 1);
    }

    #[test]
    fn infix_predicate_does_not_match_letters_inside_a_word() {
        // `textNode` / `axisLocked` contain the letters `is`/`node` but no
        // capitalized predicate *word* boundary, so they still require a prefix;
        // strictness is preserved for plain unprefixed adjective/state names.
        assert_eq!(run("let textNode: boolean = true;").len(), 1);
        assert_eq!(run("let axisLocked: boolean = true;").len(), 1);
        assert_eq!(run("let single: boolean = true;").len(), 1);
        assert_eq!(run("let visible: boolean = true;").len(), 1);
    }

    #[test]
    fn infix_predicate_still_flags_negative_phrasing() {
        // The negative-substring check runs first: `valueIsNotSet` embeds a
        // negation and is flagged as negatively phrased, not exempted.
        let diags = run("function f(valueIsNotSet: boolean) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("negatively phrased"));
    }

    #[test]
    fn no_negation_flag_on_not_infix_within_word() {
        // `Not` inside a longer word (`Notified`, the past participle of
        // "notify") is not a negation. With a valid prefix these are not flagged
        // at all; the raw issue name `abortNotified` may still trigger the
        // separate missing-prefix diagnostic, but never "negatively phrased".
        // (Closes #7027)
        assert!(run("let isNotified = false;").is_empty());
        assert!(run("let hasNotifiedListeners = false;").is_empty());
        let diags = run("let abortNotified = false;");
        assert!(!diags.iter().any(|d| d.message.contains("negatively phrased")));
    }

    #[test]
    fn still_flags_negation_at_camelcase_word_boundary() {
        // Strictness preserved: `Not`/`Cannot` at a camelCase word boundary (the
        // next character is uppercase or end-of-name) is a genuine negation and
        // still flags as negatively phrased.
        for src in [
            "let isNotSet = false;",
            "let valueIsNotFound = false;",
            "let abortNotReady = false;",
            "let valueCannotProceed = false;",
        ] {
            let diags = run(src);
            assert_eq!(diags.len(), 1, "{src}");
            assert!(diags[0].message.contains("negatively phrased"), "{src}");
        }
    }

    #[test]
    fn no_fp_on_plural_copula_are_prefix() {
        // `are` is the plural copula predicate prefix (`areMutuallyExclusive`
        // reads "the two fields are mutually exclusive"), grammatically the
        // plural-subject counterpart of `is`. (Closes #5588)
        assert!(run("function f(areMutuallyExclusive: boolean) {}").is_empty());
        assert!(run("function g(parentFieldsAreMutuallyExclusive: boolean) {}").is_empty());
        assert!(run("const areEqual: boolean = true;").is_empty());
        assert!(run("let areCompatible = false;").is_empty());
    }

    #[test]
    fn are_prefix_requires_camelcase_boundary() {
        // Strictness preserved: a noun whose name merely begins with the letters
        // `are` (`area`, `arena`) is not a predicate — the prefix must be
        // followed by an uppercase letter, so these strip to a lowercase
        // remainder and still flag.
        assert_eq!(run("let area: boolean = true;").len(), 1);
        assert_eq!(run("let arena: boolean = false;").len(), 1);
        assert_eq!(run("function f(arenaCount: boolean) {}").len(), 1);
    }

    #[test]
    fn no_fp_on_needs_necessity_prefix() {
        // `needs` is a necessity/modal predicate prefix (`needsBarrier` reads
        // "does this need a barrier?"), the same boolean-question form as the
        // allowed `should`/`will`. (Closes #5857)
        assert!(run("var needsBarrier: boolean = true;").is_empty());
        assert!(run("const needsRefresh: boolean = false;").is_empty());
        assert!(run("function f(needsUpdate: boolean) {}").is_empty());
    }

    #[test]
    fn needs_prefix_requires_camelcase_boundary() {
        // Strictness preserved: a noun whose name merely begins with the letters
        // `needs` (`needsful`-style lowercase remainders) is not a predicate —
        // the prefix must be followed by an uppercase letter, and unprefixed
        // adjective/state booleans still flag.
        assert_eq!(run("let needsful: boolean = true;").len(), 1);
        assert_eq!(run("let barrier: boolean = true;").len(), 1);
        assert_eq!(run("function f(barrier: boolean) {}").len(), 1);
    }

    #[test]
    fn are_infix_requires_subject_and_descriptor_boundary() {
        // The plural-copula infix `Are` needs a subject before and a
        // capitalized descriptor right after (`parentFieldsAreEqual`). A
        // `...Are<lowercase>` boundary (`compareAreas`) is the noun `Areas`, not
        // the copula, so it still flags; strictness is preserved.
        assert!(run("function f(parentFieldsAreEqual: boolean) {}").is_empty());
        assert_eq!(run("let compareAreas: boolean = true;").len(), 1);
    }

    #[test]
    fn no_fp_on_type_only_signature_param() {
        // A boolean parameter in a type-only callable signature (interface method
        // signature, function type, call/construct signature, abstract method,
        // function overload signature) declares NO runtime binding — its name is a
        // pure type-level contract, not a variable to name with a predicate
        // prefix. The OpenFeature `getBooleanValue(flagKey, defaultValue: boolean)`
        // signature is mandated uniformly across the boolean/string/number/object
        // overloads. (Closes #5853)
        assert!(
            run("interface Client { getBooleanValue(flagKey: string, defaultValue: boolean, options?: object): boolean; }")
                .is_empty()
        );
        assert!(run("type F = (defaultValue: boolean) => boolean;").is_empty());
        // A plain unprefixed adjective param (`verbose`) — which still flags in a
        // runtime function body — is exempt purely by the type-only position, so
        // the exemption is name-free, not a `defaultValue` allowlist.
        assert!(run("interface I { m(verbose: boolean): void; }").is_empty());
        assert!(
            run("abstract class C { abstract getBooleanValue(defaultValue: boolean): boolean; }")
                .is_empty()
        );
        // A function overload signature (bodiless) is also a type-only declaration.
        assert!(run("declare function f(defaultValue: boolean): boolean;").is_empty());
    }

    #[test]
    fn still_flags_required_boolean_param_in_runtime_function() {
        // Strictness preserved: a required boolean parameter in a concrete
        // (body-bearing) function or method is a real runtime binding and still
        // requires a predicate prefix — the type-only exemption is anchored on the
        // callable shape, not on the parameter name.
        assert_eq!(
            run("class C { getBooleanValue(defaultValue: boolean): boolean { return defaultValue; } }").len(),
            1
        );
        assert_eq!(run("function f(defaultValue: boolean): boolean { return defaultValue; }").len(), 1);
        assert_eq!(run("function f(verbose: boolean) { if (verbose) {} }").len(), 1);
        // An arrow function is a runtime binding site too.
        assert_eq!(run("const f = (verbose: boolean) => { if (verbose) {} };").len(), 1);
    }

    #[test]
    fn no_fp_on_non_boolean_union_sentinel_variable() {
        // A `string | false` / `AstObject | false` union variable holds string /
        // object content OR a `false` sentinel — it is a value-or-sentinel
        // pattern, not a boolean predicate. The `= false` initializer is the
        // sentinel, not a boolean, so a predicate prefix would be misleading.
        // (Closes #5967)
        assert!(run("let trimLeftOfNextStr: string | false = false;").is_empty());
        assert!(run("let currentObj: AstObject | false = false;").is_empty());
        assert!(run("let idx: number | false = false;").is_empty());
        // The non-boolean member may be on either side of the union.
        assert!(run("let result: false | string = false;").is_empty());
    }

    #[test]
    fn still_flags_pure_boolean_and_inferred_boolean() {
        // Strictness preserved: a pure `boolean` annotation, an unannotated
        // `= false` (inferred boolean), and a union built only from boolean-ish
        // members (`boolean | undefined`) are all genuine booleans and still
        // require a predicate prefix — only a union with a non-boolean member is
        // exempt.
        assert_eq!(run("let active: boolean = false;").len(), 1);
        assert_eq!(run("let active = false;").len(), 1);
        assert_eq!(run("let active: boolean | undefined = false;").len(), 1);
    }

    #[test]
    fn no_fp_on_define_property_descriptor_shorthand_param() {
        // `enumerable` / `writable` / `configurable` are the boolean attributes of
        // an ECMAScript property descriptor (§6.2.6). A wrapper forwarding them as
        // shorthand properties into `Object.defineProperty` is bound to their exact
        // spelling — the shorthand forces identifier == key — so an `is*` prefix
        // would break the descriptor contract. (Closes #6049)
        assert!(
            run("const dpew = (obj: any, attr: string, enumerable: boolean, writable: boolean): any =>\n  Object.defineProperty(obj, attr, {\n    enumerable,\n    writable,\n  });")
                .is_empty()
        );
        // `Reflect.defineProperty` mirrors the same descriptor-argument signature.
        assert!(
            run("function dp(obj: any, key: string, configurable: boolean) {\n  Reflect.defineProperty(obj, key, { configurable });\n}")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_descriptor_name_without_define_property_use_site() {
        // Strictness preserved — the exemption is the use-site shape, not the name:
        // an `enumerable`-named boolean parameter that is not forwarded as a
        // shorthand descriptor property still requires a predicate prefix.
        assert_eq!(run("function f(enumerable: boolean) { if (enumerable) {} }").len(), 1);
        // A shorthand descriptor key in a non-`defineProperty` call still flags.
        assert_eq!(run("function f(writable: boolean) { foo({ writable }); }").len(), 1);
        // Only the descriptor (third) argument is the forcing position — a
        // shorthand in the target (first) argument still flags.
        assert_eq!(
            run("function f(writable: boolean) { Object.defineProperty({ writable }, k, v); }").len(),
            1
        );
        // Non-shorthand forwarding (`{ enumerable: enumerable }`) does not force
        // the name, so the parameter still needs a prefix.
        assert_eq!(
            run("function f(enumerable: boolean) { Object.defineProperty(o, k, { enumerable: enumerable }); }").len(),
            1
        );
        // The plain-variable form is unaffected by the parameter exemption.
        assert_eq!(run("const enumerable: boolean = true;").len(), 1);
    }

    #[test]
    fn no_fp_on_opaque_non_boolean_annotation_with_bool_init() {
        // An explicit annotation whose top-level type is neither `boolean` nor a
        // union — an indexed-access type (`Listener["https"]` resolves to
        // `false | Certificate`), a type reference (`HTTPSCert`) — is an opaque
        // non-boolean type and is the source of truth. A `= false` / `= true`
        // initializer is a sentinel value, not a boolean predicate, so it must
        // not be flagged based solely on its init value. (Closes #6641)
        assert!(run("let https: Listener[\"https\"] = false;").is_empty());
        assert!(run("let cert: HTTPSCert = false;").is_empty());
        assert!(run("let x: SomeType = false;").is_empty());
        assert!(run("let y: Listener[\"https\"] = true;").is_empty());
    }

    #[test]
    fn still_flags_annotated_and_unannotated_real_booleans() {
        // Strictness preserved: the opaque-annotation exemption only suppresses
        // the init-value heuristic for non-boolean annotations. A genuine
        // `boolean` annotation, a boolean-ish union (`boolean | undefined`), and
        // an unannotated `= false` (inferred boolean) are all real booleans and
        // still require a predicate prefix.
        assert_eq!(run("let done: boolean = false;").len(), 1);
        assert_eq!(run("let ready: boolean | undefined = false;").len(), 1);
        assert_eq!(run("let active = false;").len(), 1);
    }
}
