use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

/// Return a 4-character lowercase prefix bucket for `name`, so close
/// variants such as `cancelReason` and `cancelledAt` collide on the
/// same bucket (`canc`). Returns the empty string when the name has
/// fewer than 4 leading ASCII alphabetic characters.
fn leading_prefix(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut buf = String::with_capacity(4);
    for &b in bytes.iter().take(4) {
        if !b.is_ascii_alphabetic() {
            return String::new();
        }
        buf.push(b.to_ascii_lowercase() as char);
    }
    if buf.len() < 4 {
        return String::new();
    }
    buf
}

/// Whether `ty` is a function type — an arrow signature `(...) => T`
/// (`TSFunctionType`) or a constructor signature `new (...) => T`
/// (`TSConstructorType`). An optional member typed as a function is a
/// callback / event-handler / visitor hook (`visitProgram?: (ctx) => Result`,
/// `onClick?: () => void`), independently overridable, never a
/// mutually-exclusive DATA variant — so it must not feed the cluster.
fn is_function_type(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::TSType;
    matches!(ty, TSType::TSFunctionType(_) | TSType::TSConstructorType(_))
}

/// Whether `name` is a DOM/React event-handler prop (`on` followed by an
/// uppercase letter, e.g. `onClick`, `onMouseEnter`). These are independent
/// callbacks that share the `on*` prefix by naming convention — a component
/// can wire up `onMouseEnter`, `onMouseMove` and `onMouseLeave` together — so
/// they never encode mutually-exclusive state and must not feed a cluster.
fn is_event_handler_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() > 2 && bytes[0] == b'o' && bytes[1] == b'n' && bytes[2].is_ascii_uppercase()
}

/// Prefix buckets that name a React/UI concept rather than a state
/// machine, so a cluster sharing one is a semantic grouping, not an
/// optional-flag state encoding:
/// - `defa` — idiomatic uncontrolled-component props (`defaultValue`,
///   `defaultActiveId`, `defaultChecked`, `defaultOpen`): independent
///   initial-state props, not mutually-exclusive variants.
/// - `ente` / `leav` — Headless UI / Tailwind animation phases
///   (`enter`/`enterFrom`/`enterTo`, `leave`/`leaveFrom`/`leaveTo`):
///   all apply simultaneously to describe one transition.
fn is_semantic_grouping_prefix(prefix: &str) -> bool {
    matches!(prefix, "defa" | "ente" | "leav")
}

/// Whether `type_name` follows a configuration naming convention
/// (`FooConfig`, `BarOptions`, `BazSettings`, …). A configuration type is
/// a bag of independent tunable knobs, each optional because it has a
/// default and can be set in any combination — there is no mutual
/// exclusion and no encoded state machine. Optional fields there may share
/// a vocabulary prefix (`customResolveInfo`/`customResolverFn` → `cust`)
/// without modeling a variant, so prefix-clustering carries no signal and
/// the cluster heuristic is suppressed. `*Rules` is a per-field knob bag of
/// the same shape (one independent `Rule` per protocol field, e.g. an
/// EIP-4337 `UserOperationRules` mapping each spec field to a validation
/// rule), so it joins the convention.
fn is_configuration_type_name(type_name: &str) -> bool {
    const CONFIG_SUFFIXES: [&str; 9] = [
        "Config",
        "Configuration",
        "Options",
        "Opts",
        "Settings",
        "Props",
        "Params",
        "Args",
        "Rules",
    ];
    CONFIG_SUFFIXES.iter().any(|suffix| type_name.ends_with(suffix))
}

/// Axis / direction suffixes that, appended to a bare base field, name a
/// CSS-inspired shorthand + per-axis property family: axes (`X`/`Y`/`Z`/
/// `3d`), box sides (`Top`/`Right`/`Bottom`/`Left`) and logical edges
/// (`Start`/`End`/`Inline`/`Block`). Case-sensitive: the suffix is the
/// capitalized tail of a camelCase name (`translateX`), with `3d`/`3D`
/// accepted for the depth axis.
fn is_axis_suffix(suffix: &str) -> bool {
    matches!(
        suffix,
        "X" | "Y"
            | "Z"
            | "3d"
            | "3D"
            | "Top"
            | "Right"
            | "Bottom"
            | "Left"
            | "Start"
            | "End"
            | "Inline"
            | "Block"
    )
}

/// Geometric axis suffixes that, appended to a shared stem, name one of a set
/// of co-occurring coordinate or dimension quantities rather than a mutually-
/// exclusive variant: spatial axes (`X`/`Y`/`Z`) and box dimensions
/// (`Width`/`Height`/`Depth`).
const GEOMETRIC_AXIS_SUFFIXES: [&str; 6] = ["Width", "Height", "Depth", "X", "Y", "Z"];

/// Strip a trailing geometric axis suffix from `name`, returning its stem.
/// The suffix must be a recognized geometric axis sitting at a camelCase
/// boundary: it follows a lowercase letter, so a single-letter axis is not
/// torn off a mid-word capital and the stem stays a real word (`pageX` → `page`,
/// `parentWidth` → `parent`; `index` keeps its lowercase `x`). Returns `None`
/// when no recognized suffix follows a lowercase letter.
fn split_geometric_axis(name: &str) -> Option<&str> {
    for suffix in GEOMETRIC_AXIS_SUFFIXES {
        if let Some(stem) = name.strip_suffix(suffix) {
            let preceded_by_lowercase =
                stem.chars().next_back().is_some_and(|c| c.is_ascii_lowercase());
            if preceded_by_lowercase {
                return Some(stem);
            }
        }
    }
    None
}

/// Whether `name` is one of a geometric axis-pair group in `names`: it ends
/// in a recognized geometric axis suffix and a sibling shares its stem with
/// a *different* geometric axis suffix (`pageX`/`pageY`,
/// `parentWidth`/`parentHeight`). Such fields are co-occurring coordinate or
/// dimension quantities, not mutually-exclusive variant states, so they must
/// not contribute to a variant cluster. A lone suffixed field with no
/// matching-stem sibling (`pageX` alone) is not a pair and still counts.
fn is_in_geometric_axis_pair(name: &str, names: &[&str]) -> bool {
    let Some(stem) = split_geometric_axis(name) else {
        return false;
    };
    names.iter().any(|other| {
        *other != name && split_geometric_axis(other).is_some_and(|other_stem| other_stem == stem)
    })
}

/// Count the bucket members that still look like a discriminated-union
/// state cluster, i.e. the members that are neither part of a CSS-inspired
/// shorthand + per-axis family, nor in a base + elaboration prefix pair, nor
/// in a geometric axis-pair group.
///
/// A shorthand + per-axis family is a bare base field `F` (the shorthand,
/// e.g. `translate`, `extrapolate`) together with the siblings formed by
/// appending a recognized axis/direction suffix (`translateX`,
/// `extrapolateLeft`). Those are composable — a consumer can set the
/// shorthand or the individual axes independently — so they are excluded
/// from the cluster count. The bare base must be present: a bucket of
/// only suffixed siblings (`translateX`/`translateY`, no `translate`)
/// lacks the shorthand signature and every member still counts.
///
/// A base + elaboration pair is a member that is a strict prefix of, or
/// strictly prefixed by, another bucket member (`sources`/`sourcesContent`,
/// `version`/`versionId`). One name extends the other, so they are an
/// independent base + detail pair, not a common stem with mutually-exclusive
/// suffixes — both can be present, so they are excluded from the count.
/// A genuine variant (`cancelledAt`/`cancelledReason`) shares a stem but
/// neither name is a prefix of the other, so every member still counts.
///
/// A geometric axis-pair group is two or more members sharing a stem and
/// differing only by a geometric axis suffix (`pageX`/`pageY`,
/// `parentWidth`/`parentHeight`). They are co-occurring coordinate or
/// dimension quantities, idiomatically optional together, not mutually-
/// exclusive variants — so they are excluded from the count.
///
/// Members sharing the prefix but belonging to no family (e.g. `transform`
/// alongside the `translate*` family in the `tran` bucket) keep counting,
/// so an unrelated state cluster is not masked by an adjacent family.
fn variant_field_count(names: &[&str]) -> usize {
    names
        .iter()
        .filter(|name| {
            !is_in_shorthand_axis_family(name, names)
                && !is_in_elaboration_pair(name, names)
                && !is_in_geometric_axis_pair(name, names)
        })
        .count()
}

/// Whether `name` is a strict prefix of, or strictly prefixed by, another
/// bucket member — a base + elaboration relationship (`sources` extended to
/// `sourcesContent`), not a discriminated variant.
fn is_in_elaboration_pair(name: &str, names: &[&str]) -> bool {
    names
        .iter()
        .any(|other| *other != name && (other.starts_with(name) || name.starts_with(other)))
}

/// Whether `name` is the bare base of, or a suffixed sibling in, a
/// shorthand + per-axis family present in `names`. A family requires a
/// bare base `F` ∈ `names` and at least one sibling `F + <axis suffix>`
/// also ∈ `names`.
fn is_in_shorthand_axis_family(name: &str, names: &[&str]) -> bool {
    let has_family = |base: &str| -> bool {
        names.iter().any(|other| *other != base && other.strip_prefix(base).is_some_and(is_axis_suffix))
    };
    // `name` is itself a bare base with at least one suffixed sibling.
    if has_family(name) {
        return true;
    }
    // `name` is a suffixed sibling of some bare base present in `names`.
    names
        .iter()
        .any(|base| name.strip_prefix(*base).is_some_and(is_axis_suffix) && has_family(base))
}

/// Whether `members` contains a non-optional property signature typed as a
/// string-literal type (`type: 'spring'`). A required literal-typed member is
/// the structural marker of a resolved discriminated-union variant: the
/// interface is one arm of a union keyed on this field, so its optional fields
/// are independent per-variant configuration knobs, not an encoded sub-state
/// machine — the discriminated-union refactor advice is nonsensical. The field
/// name is irrelevant; any required string-literal-typed member is the signal.
/// (Regression: #6364.)
fn has_required_literal_discriminant(
    members: &oxc_allocator::Vec<'_, oxc_ast::ast::TSSignature<'_>>,
) -> bool {
    use oxc_ast::ast::{TSLiteral, TSType};
    members.iter().any(|member| {
        let oxc_ast::ast::TSSignature::TSPropertySignature(prop) = member else {
            return false;
        };
        if prop.optional {
            return false;
        }
        let Some(annot) = &prop.type_annotation else {
            return false;
        };
        matches!(
            &annot.type_annotation,
            TSType::TSLiteralType(lit) if matches!(&lit.literal, TSLiteral::StringLiteral(_))
        )
    })
}

fn collect_optional_prefixes<'b>(
    members: &'b oxc_allocator::Vec<'_, oxc_ast::ast::TSSignature<'_>>,
) -> FxHashMap<String, Vec<&'b str>> {
    let mut buckets: FxHashMap<String, Vec<&'b str>> = FxHashMap::default();
    for member in members.iter() {
        let oxc_ast::ast::TSSignature::TSPropertySignature(prop) = member else {
            continue;
        };
        if !prop.optional {
            continue;
        }
        if let Some(annot) = &prop.type_annotation {
            // Skip phantom / mutually-exclusive-props patterns where the
            // annotation is `never` — those keys MUST be absent, opposite
            // of an optional state flag. (Regression: #120.)
            if matches!(annot.type_annotation, oxc_ast::ast::TSType::TSNeverKeyword(_)) {
                continue;
            }
            // Skip function-typed members — callbacks / event handlers /
            // visitor hooks (`visitProgram?: (ctx) => Result`) are
            // independently overridable, not mutually-exclusive DATA
            // variants, so they carry no discriminated-union signal.
            // (Regression: #4698.)
            if is_function_type(&annot.type_annotation) {
                continue;
            }
        }
        let name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            _ => continue,
        };
        // Skip DOM/React event-handler props (`onClick`, `onMouseEnter`):
        // independent callbacks sharing the `on*` convention, never a
        // state-variant cluster. (Regression: #4776.)
        if is_event_handler_name(name) {
            continue;
        }
        let prefix = leading_prefix(name);
        if prefix.len() < 4 {
            continue;
        }
        if is_semantic_grouping_prefix(&prefix) {
            continue;
        }
        buckets.entry(prefix).or_default().push(name);
    }
    buckets
}

fn check_optional_clusters(
    buckets: FxHashMap<String, Vec<&str>>,
    type_name: &str,
    span_start: u32,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if is_configuration_type_name(type_name) {
        return;
    }
    let mut hits: Vec<(&String, usize)> = buckets
        .iter()
        .map(|(prefix, names)| (prefix, variant_field_count(names)))
        .filter(|(_, count)| *count >= 2)
        .collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_by(|a, b| b.1.cmp(&a.1));
    let (prefix, count) = &hits[0];
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "`{type_name}` has {count} optional fields sharing prefix `{prefix}\u{2026}` \u{2014} encode this state with a discriminated union instead."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration, AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Generated files (e.g. the ANTLR `// Generated from … by ANTLR`
        // banner) declare machine-emitted interfaces — a discriminated-union
        // refactor is not actionable. (Regression: #4698.)
        if ctx.file.is_generated {
            return;
        }
        // Module augmentations (`declare module 'foo' { ... }`) are not API
        // response types — optional fields there are intentional metadata.
        if crate::oxc_helpers::is_in_ambient_declaration(node.id(), semantic) {
            return;
        }
        match node.kind() {
            AstKind::TSInterfaceDeclaration(iface) => {
                // An interface carrying a required string-literal discriminant
                // (`type: 'spring'`) is an already-resolved discriminated-union
                // variant, not a state-machine host. (Regression: #6364.)
                if has_required_literal_discriminant(&iface.body.body) {
                    return;
                }
                let name = iface.id.name.as_str();
                let counts = collect_optional_prefixes(&iface.body.body);
                check_optional_clusters(counts, name, iface.span.start, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(alias) => {
                let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &alias.type_annotation else {
                    return;
                };
                let name = alias.id.name.as_str();
                let counts = collect_optional_prefixes(&lit.members);
                check_optional_clusters(counts, name, alias.span.start, ctx, diagnostics);
            }
            _ => {}
        }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_two_optional_fields_sharing_prefix() {
        let src = "interface Order { id: string; cancelReason?: string; cancelledAt?: string }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_phantom_never_props() {
        // Regression for #120: `{ page?: never; pageSize?: never; q?: never; sort?: never }`
        // is a mutually-exclusive-props / phantom-key pattern. `?: never`
        // declares the key MUST be absent — opposite of an optional
        // state flag, so the cluster heuristic must skip it.
        let src = "type Phantom = { page?: never; pageSize?: never; q?: never; sort?: never };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_default_prefixed_react_props() {
        // Regression for #1786: `default*` props are the idiomatic React
        // uncontrolled-component API (`defaultValue`, `defaultActiveId`),
        // independent initial-state props, not a state-variant cluster.
        let src = r#"export interface AccordionProps {
  children?: React.ReactNode
  className?: string
  defaultActiveId?: (string | number)[]
  onChange?: (item: string | string[]) => void
  openBehaviour: 'single' | 'multiple'
  defaultValue?: string | string[] | undefined
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_animation_phase_groupings() {
        // Regression for #1786: Headless UI / Tailwind animation phases
        // (`enter`/`enterFrom`/`enterTo`, `leave`/`leaveFrom`/`leaveTo`)
        // all apply simultaneously to describe one transition, not
        // mutually-exclusive state variants.
        let src = r#"export interface AnimationTailwindClasses {
  enter?: string
  enterFrom?: string
  enterTo?: string
  leave?: string
  leaveFrom?: string
  leaveTo?: string
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_genuine_state_cluster() {
        // The exemption is prefix-specific: a real optional-flag state
        // cluster must still be flagged.
        let src = "interface Order { id: string; shipReason?: string; shipmentAt?: string }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_translate_shorthand_axis_family() {
        // Regression for #2190: a CSS-inspired shorthand (`translate`) plus
        // per-axis siblings (`translateX`/`translateY`/`translateZ`/
        // `translate3d`) is composable, not a discriminated-union state
        // cluster — a consumer can set the shorthand or individual axes.
        let src = r#"type TransformProps = {
  transform?: string
  translate?: Length
  translateX?: Length
  translateY?: Length
  translateZ?: Length
  translate3d?: readonly [Length, Length, Length]
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_extrapolate_shorthand_direction_family() {
        // Regression for #2190: `extrapolate` is a shorthand that sets both
        // `extrapolateLeft` and `extrapolateRight`; the per-direction props
        // are independent, not mutually-exclusive variants.
        let src = r#"export type InterpolatorConfig = {
  extrapolate?: ExtrapolateType
  extrapolateLeft?: ExtrapolateType
  extrapolateRight?: ExtrapolateType
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_variant_group_without_shorthand_base() {
        // The shorthand+axis exemption requires a bare base field whose
        // siblings are `base + <axis suffix>`. A genuine state cluster that
        // merely shares a 4-char prefix (`shipReason`/`shipmentAt`, bucket
        // `ship`) has no bare base + axis siblings and stays flagged.
        let src = "type Order = { shipReason?: string; shipmentAt?: string; shippedTo?: string };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_geometric_axis_siblings_without_shorthand_base() {
        // Regression for #4862: per-axis siblings sharing a stem and differing
        // only by an `X`/`Y`/`Z` suffix (`translateX`/`translateY`/`translateZ`)
        // are a geometric axis-pair group — co-occurring coordinate quantities,
        // not mutually-exclusive variants — so they are exempt even with no
        // bare `translate` shorthand base.
        let src = "type T = { translateX?: number; translateY?: number; translateZ?: number };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_prefix_of_other_elaboration_pair() {
        // Regression for #2082: in the Source Map V3 spec, `sources` and
        // `sourcesContent` share the `sour` bucket, but `sources` is a strict
        // prefix of `sourcesContent` — a base + elaboration relationship
        // (independent, both-can-be-present fields), not a common stem with
        // mutually-exclusive suffixes. They must not be flagged.
        let src = r#"export interface SourceMap {
  file?: string
  mappings?: string
  names?: string[]
  sources?: string[]
  sourcesContent?: string[]
  version?: number
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_variant_without_prefix_of_other() {
        // Negative-space guard for #2082: `cancelledAt` / `cancelledReason`
        // share the stem `cancelled` with different, mutually-exclusive
        // suffixes; neither is a prefix of the other, so the cluster is a
        // genuine discriminated-union smell and stays flagged.
        let src = "interface Order { id: string; cancelledAt?: string; cancelledReason?: string }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_config_interface_vocabulary_prefix_cluster() {
        // Regression for #3270: in a configuration interface (`*Config`),
        // optional fields are independent tunable knobs that legitimately
        // share a vocabulary prefix (`customResolveInfo`/`customResolverFn`
        // → `cust`; a heterogeneous `resolver…` cluster → `reso`). They are
        // not variants of a state machine, so the prefix cluster must not be
        // flagged. The `reso` cluster mixes string/string/boolean, so a pure
        // type-homogeneity test would not clear it — config-name suppression
        // is the load-bearing rule.
        let src = r#"export interface TypeScriptResolversPluginConfig {
  customResolveInfo?: string;
  customResolverFn?: string;
  futureProofEnums?: boolean;
  futureProofUnions?: boolean;
  resolverTypeWrapperSignature?: string;
  resolverTypeSuffix?: string;
  resolversNonOptionalTypename?: boolean;
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_options_type_alias_vocabulary_prefix_cluster() {
        // Regression for #3270: the config-name convention covers type
        // aliases ending in `Options` too — independent knobs, not variants.
        let src = r#"type DocumentOptions = {
  documentMode?: string;
  documentTransformTypeName?: string;
};"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_eip4337_user_operation_rules_type() {
        // Regression for #4826: an EIP-4337 UserOperation `*Rules` type maps
        // each spec-mandated protocol field to an independent validation
        // `Rule`. `callData`/`callGasLimit` share the `call` bucket only
        // because both name aspects of the same EVM call, not because they
        // are mutually-exclusive variant states — every field can be present
        // at once. `Rules` is a per-field knob bag, so the config-name
        // convention suppresses the cluster.
        let src = r#"export type UserOperationV06Rules = {
  sender?: Rule;
  nonce?: Rule;
  initCode?: Rule;
  callData?: Rule;
  callGasLimit?: Rule;
  verificationGasLimit?: Rule;
  preVerificationGas?: Rule;
  maxFeePerGas?: Rule;
  maxPriorityFeePerGas?: Rule;
  paymasterAndData?: Rule;
  chainId?: Rule;
  entrypoint?: Rule;
};"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_state_cluster_in_non_rules_type() {
        // Guardrail for #4826: the `Rules` suffix suppression must not leak
        // into ordinary domain types. A genuine mutually-exclusive
        // optional-flag cluster (bucket `load`) in a non-config interface
        // stays flagged.
        let src = r#"interface RequestStatus {
  loadingPending?: boolean;
  loadedAt?: boolean;
  loadFailed?: boolean;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_domain_state_cluster_in_non_config_interface() {
        // Guardrail for #3270: a genuine heterogeneous domain-state cluster
        // in a NON-config interface (`Order`, not `*Config`/`*Options`) must
        // still be flagged — the config-name suppression must not leak into
        // ordinary domain types.
        let src = r#"interface Order {
  cancelReason?: string;
  cancelledAt?: Date;
  cancelledBy?: User;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_dom_event_handler_props() {
        // Regression for #4776: independent DOM/React event-handler callbacks
        // (`onMouseEnter`/`onMouseMove`/`onMouseLeave`, bucket `onmo`) share
        // the `on*` prefix by naming convention, not mutual exclusion — a
        // component wires up all three together. They must not be flagged.
        let src = r#"export type MouseHandlers<Datum> = {
  onClick?: MouseHandler<Datum>
  onMouseEnter?: MouseHandler<Datum>
  onMouseMove?: MouseHandler<Datum>
  onMouseLeave?: MouseHandler<Datum>
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_state_cluster_alongside_event_handlers() {
        // Negative-space guard for #4776: dropping event handlers from the
        // cluster count must not mask a genuine state cluster declared in the
        // same type — `cancelReason`/`cancelledAt` (bucket `canc`) stays flagged.
        let src = r#"interface Order {
  onClick?: () => void;
  onMouseEnter?: () => void;
  cancelReason?: string;
  cancelledAt?: Date;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_on_prefixed_non_handler_cluster() {
        // Negative-space guard for #4776: the exemption is `on` + an uppercase
        // letter, not `on` + anything. A genuine cluster whose names merely
        // start with `on` followed by a lowercase letter (`onlineSince`/
        // `onlineUntil`, bucket `onli`) is not an event handler and stays flagged.
        let src = "interface Session { onlineSince?: Date; onlineUntil?: Date }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_antlr_visitor_interface_function_typed_hooks() {
        // Regression for #4698: an ANTLR4-generated visitor interface declares
        // one optional `visit*` member per grammar rule, each typed as an arrow
        // signature `(ctx) => Result`. These are independent, selectively
        // overridable hooks (visitor pattern), not mutually-exclusive DATA
        // variants — the discriminated-union refactor is nonsensical. Function-
        // typed optional members must not feed the cluster.
        let src = r#"export interface TSQLParserVisitor<Result> {
  visitProgram?: (ctx: ProgramContext) => Result;
  visitDeclaration?: (ctx: DeclarationContext) => Result;
  visitVarDecl?: (ctx: VarDeclContext) => Result;
  visitFunctionDef?: (ctx: FunctionDefContext) => Result;
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_hand_written_handler_interface_function_typed() {
        // Regression for #4698: the function-type discriminator is general, so
        // a hand-written event-handler interface whose optional members are
        // arrow types (`handleA?: () => void`, bucket `hand`) is also exempt —
        // independent handlers, not data variants. The `on*` exemption already
        // covers `onA`/`onB`, so `handle*` exercises the type-shape signal.
        let src = r#"interface Handlers {
  handleOpen?: () => void;
  handleClose?: () => void;
  handleToggle?: () => void;
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_data_field_variant_cluster_among_function_fields() {
        // Negative-space guard for #4698: dropping function-typed members from
        // the cluster must not mask a genuine DATA-field variant smell declared
        // in the same type — `cancelReason?`/`cancelledAt?` (bucket `canc`,
        // string/Date data) stays flagged alongside arrow-typed handlers.
        let src = r#"interface Order {
  handleOpen?: () => void;
  handleClose?: () => void;
  cancelReason?: string;
  cancelledAt?: Date;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_data_field_cluster_sharing_visit_prefix() {
        // Negative-space guard for #4698: the exemption is the function-type
        // shape, NOT the `visit` name. A DATA-field cluster that merely shares
        // the `visi` bucket (`visibleFrom?: Date`/`visitedAt?: Date`) is a
        // genuine optional-flag smell and stays flagged.
        let src = "interface Page { visibleFrom?: Date; visitedAt?: Date; visualMode?: string }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_coordinate_axis_pair() {
        // Regression for #4862: independent optional coordinate axes
        // (`pageX`/`pageY`, bucket `page`) are co-occurring geometric
        // quantities, not mutually-exclusive variant states.
        let src = "interface Pos { pageX?: number; pageY?: number }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_dimension_axis_pair() {
        // Regression for #4862: `parentWidth`/`parentHeight` (bucket `pare`)
        // share a stem and differ only by a geometric dimension suffix —
        // co-occurring quantities, not a state machine.
        let src = "interface Box { parentWidth?: number; parentHeight?: number }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_offset_axis_pair() {
        // Regression for #4862: `offsetX`/`offsetY` (bucket `offs`) is an
        // axis pair keyed off the `X`/`Y` suffix vocabulary, not a hardcoded
        // field-name list.
        let src = "interface E { offsetX?: number; offsetY?: number }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_short_width_height_pair() {
        // Regression for #4862: bare `width`/`height` are geometric
        // dimensions. (They land in distinct prefix buckets so already do
        // not cluster, but the axis-pair shape must still hold.)
        let src = "interface Size { width?: number; height?: number }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_genuine_mutually_exclusive_cluster() {
        // Negative-space guard for #4862: a real mutually-exclusive status
        // cluster sharing a 4-char prefix (`loadingState`/`loadedState`/
        // `loadFailedState`, bucket `load`) is unaffected by the axis-pair
        // exemption — none of these end in a geometric axis suffix.
        let src = r#"interface State {
  loadingState?: boolean;
  loadedState?: boolean;
  loadFailedState?: boolean;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_data_variant_cluster_sharing_stem() {
        // Negative-space guard for #4862: `skipA`/`skipB`/`skipC` (bucket
        // `skip`) share a stem but the trailing tokens (`A`/`B`/`C`) are not
        // geometric axis suffixes, so the cluster is not exempt.
        let src = "type Cfg = { skipA?: string; skipB?: string; skipC?: string };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_mis_strip_uppercase_axis_suffix() {
        // Guardrail for #4862: an axis token is only stripped when it is the
        // trailing camelCase segment. `boxShadow`/`boxSizing` (bucket `boxs`)
        // end in lowercase letters, so neither is mistaken for an axis pair
        // and this genuine cluster stays flagged.
        let src = "interface S { boxShadow?: string; boxSizing?: string }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_interface_with_required_string_literal_discriminant() {
        // Regression for #6364: `Spring` is a member of a discriminated union
        // (`type PopmotionTransitionProps = Spring | Inertia | …`) keyed on its
        // required `type: 'spring'` literal. The prefix-sharing optional fields
        // (`restSpeed`/`restDelta`, bucket `rest`) are independent physics knobs,
        // not mutually-exclusive sub-states — the required string-literal
        // discriminant marks the interface as an already-resolved variant.
        let src = r#"interface Spring {
  type: 'spring'
  stiffness?: number
  damping?: number
  restSpeed?: number
  restDelta?: number
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_cluster_without_required_literal_discriminant() {
        // Negative-space guard for #6364: the discriminant skip is structural,
        // keyed on a required string-LITERAL-typed member. An interface with no
        // such member but with a genuine prefix-sharing optional cluster
        // (`restSpeed`/`restDelta`, bucket `rest`) stays flagged.
        let src = r#"interface Spring {
  stiffness?: number
  restSpeed?: number
  restDelta?: number
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_cluster_with_optional_or_plain_string_type_field() {
        // Negative-space guard for #6364: the discriminant must be both required
        // and typed as a string LITERAL. An optional `type?: 'spring'` is not a
        // resolved discriminant, and a plain `kind: string` is not a literal —
        // neither suppresses a genuine prefix cluster (`restSpeed`/`restDelta`).
        let src = r#"interface Spring {
  type?: 'spring'
  kind: string
  restSpeed?: number
  restDelta?: number
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_declare_module_augmentation() {
        // Regression for #544: module augmentations (e.g. TanStack Router
        // StaticDataRouteOption) are not API response types; optional fields
        // there are intentional route metadata, not state-variant clusters.
        let src = r#"declare module '@tanstack/react-router' {
  interface StaticDataRouteOption {
    title?: string;
    breadcrumbParent?: string;
    breadcrumbAncestors?: { title: string; pathname: string }[];
  }
}"#;
        assert!(run_on(src).is_empty());
    }
}
