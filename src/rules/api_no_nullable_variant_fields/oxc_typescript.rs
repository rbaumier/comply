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
/// the cluster heuristic is suppressed.
fn is_configuration_type_name(type_name: &str) -> bool {
    const CONFIG_SUFFIXES: [&str; 8] =
        ["Config", "Configuration", "Options", "Opts", "Settings", "Props", "Params", "Args"];
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

/// Count the bucket members that still look like a discriminated-union
/// state cluster, i.e. the members that are neither part of a CSS-inspired
/// shorthand + per-axis family nor in a base + elaboration prefix pair.
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
/// Members sharing the prefix but belonging to no family (e.g. `transform`
/// alongside the `translate*` family in the `tran` bucket) keep counting,
/// so an unrelated state cluster is not masked by an adjacent family.
fn variant_field_count(names: &[&str]) -> usize {
    names
        .iter()
        .filter(|name| !is_in_shorthand_axis_family(name, names) && !is_in_elaboration_pair(name, names))
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
        // Skip phantom / mutually-exclusive-props patterns where the
        // annotation is `never` — those keys MUST be absent, opposite
        // of an optional state flag. (Regression: #120.)
        if let Some(annot) = &prop.type_annotation
            && matches!(annot.type_annotation, oxc_ast::ast::TSType::TSNeverKeyword(_))
        {
            continue;
        }
        let name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            _ => continue,
        };
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
        // Module augmentations (`declare module 'foo' { ... }`) are not API
        // response types — optional fields there are intentional metadata.
        if crate::oxc_helpers::is_in_ambient_declaration(node.id(), semantic) {
            return;
        }
        match node.kind() {
            AstKind::TSInterfaceDeclaration(iface) => {
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
    fn still_flags_axis_siblings_without_shorthand_base() {
        // Per-axis siblings with no bare shorthand base (`translateX`/
        // `translateY`/`translateZ`, no bare `translate`) lack the shorthand
        // signature, so the cluster is not exempt.
        let src = "type T = { translateX?: number; translateY?: number; translateZ?: number };";
        assert_eq!(run_on(src).len(), 1);
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
