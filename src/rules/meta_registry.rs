//! Boot-time lookup tables keyed by `rule_id`, built once on first call via
//! `OnceLock` from the registered rule definitions:
//!
//! - [`lookup`] → the rule's `RuleMeta`. The pretty renderer uses it to surface
//!   RuleMeta-only fields (description, remediation, doc_url) that aren't
//!   carried on `Diagnostic` itself.
//! - [`upstream_eslint_rule`] → the ESLint rule the comply rule re-implements,
//!   derived from the rule's own metadata. The suppression layer uses it to
//!   honor an inline ESLint directive that names that upstream rule.
//!
//! Delegated diagnostics (oxlint, clippy, knip, madge) carry rule ids that
//! are NOT in comply's RuleMeta catalogue — `lookup` returns `None` for
//! those, and the renderer omits the help/url sections for that diagnostic.

use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use rustc_hash::FxHashMap;
use std::sync::OnceLock;

static REGISTRY: OnceLock<FxHashMap<&'static str, RuleMeta>> = OnceLock::new();
static UPSTREAM_ESLINT: OnceLock<FxHashMap<&'static str, &'static str>> = OnceLock::new();

fn build() -> FxHashMap<&'static str, RuleMeta> {
    crate::rules::all_rule_defs()
        .into_iter()
        .map(|r| (r.meta.id, r.meta))
        .collect()
}

/// Returns the `RuleMeta` for a given rule id, or `None` if the id is
/// unknown to comply (typically a delegated oxlint/clippy id).
#[must_use]
pub fn lookup(rule_id: &str) -> Option<RuleMeta> {
    REGISTRY.get_or_init(build).get(rule_id).copied()
}

fn build_upstream_eslint() -> FxHashMap<&'static str, &'static str> {
    crate::rules::all_rule_defs_static()
        .iter()
        .filter_map(|r| {
            upstream_eslint_name(&r.meta, &r.backends).map(|name| (r.meta.id, name))
        })
        .collect()
}

/// Unprefixed name of the ESLint rule that `rule_id` re-implements
/// (`no-empty-function` for `ts-no-empty-function`), or `None` when the rule
/// has no ESLint counterpart.
///
/// The plugin prefix is dropped because a project's ESLint config decides
/// which one its directives carry — `@typescript-eslint/no-empty-function`,
/// `typescript/no-empty-function` and `no-empty-function` all name the rule
/// below. Callers match on this bare name.
#[must_use]
pub fn upstream_eslint_rule(rule_id: &str) -> Option<&'static str> {
    UPSTREAM_ESLINT
        .get_or_init(build_upstream_eslint)
        .get(rule_id)
        .copied()
}

/// Derive a rule's upstream ESLint name from its own metadata: the documented
/// rule page it points at, else the oxlint rule it delegates to. A rule that
/// declares neither is comply-original and yields `None`.
///
/// The oxlint key is a delegation target, so it names the upstream rule by
/// construction. A `doc_url` is only a link, and some rules link the ESLint
/// page of a *related* check — `prefer-early-return` cites `no-else-return`,
/// which comply also registers as its own rule. The id must therefore end in
/// the derived name, on a segment boundary, for the link to count as an
/// identity claim.
fn upstream_eslint_name(
    meta: &RuleMeta,
    backends: &[(Language, Backend)],
) -> Option<&'static str> {
    meta.doc_url
        .and_then(eslint_rule_from_doc_url)
        .filter(|name| {
            meta.id
                .strip_suffix(name)
                .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with('-'))
        })
        .or_else(|| {
            backends.iter().find_map(|(_, backend)| match backend {
                Backend::Oxlint { rule, .. } => Some(unprefixed_rule_name(rule)),
                _ => None,
            })
        })
}

/// Rule name carried by an ESLint rule documentation URL, or `None` when the
/// URL documents something else.
///
/// Every ESLint rule page — core (`eslint.org/docs/latest/rules/<name>`),
/// typescript-eslint (`typescript-eslint.io/rules/<name>/`) or a plugin
/// (`.../eslint-plugin-<x>/**/rules/<name>.md`) — ends in a `rules/` segment
/// followed by the rule name. Requiring `eslint` somewhere in the URL keeps
/// out other linters that use the same path shape (Biome, whose rule names
/// differ from ESLint's).
fn eslint_rule_from_doc_url(url: &'static str) -> Option<&'static str> {
    let path = match url.find(['#', '?']) {
        Some(cut) => &url[..cut],
        None => url,
    };
    if !path.contains("eslint") {
        return None;
    }
    let name = path.rsplit_once("/rules/")?.1.trim_end_matches('/');
    let name = name
        .strip_suffix(".md")
        .or_else(|| name.strip_suffix(".html"))
        .unwrap_or(name);
    // A plugin landing page (`.../rules/`) names no rule.
    if name.is_empty() {
        return None;
    }
    Some(name)
}

/// Strip a plugin prefix from a lint rule name: `typescript/array-type` and
/// `@next/next/no-img-element` become `array-type` and `no-img-element`. A
/// name with no prefix is returned unchanged.
#[must_use]
pub fn unprefixed_rule_name(rule: &str) -> &str {
    rule.rsplit_once('/').map_or(rule, |(_, bare)| bare)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_finds_registered_rule() {
        // "no-weak-cipher" is a real rule registered in all_rule_defs().
        let meta = lookup("no-weak-cipher").expect("no-weak-cipher must be in the registry");
        assert_eq!(meta.id, "no-weak-cipher");
    }

    #[test]
    fn lookup_returns_none_for_unknown_rule_id() {
        assert!(lookup("not-a-real-rule-id-zzz-xyz").is_none());
    }

    #[test]
    fn doc_url_yields_bare_eslint_rule_name() {
        let cases = [
            ("https://typescript-eslint.io/rules/no-empty-function/", "no-empty-function"),
            ("https://typescript-eslint.io/rules/no-unused-expressions", "no-unused-expressions"),
            ("https://eslint.org/docs/latest/rules/no-else-return", "no-else-return"),
            ("https://eslint.vuejs.org/rules/no-ref-as-operand.html", "no-ref-as-operand"),
            (
                "https://github.com/eslint-community/eslint-plugin-promise/blob/main/docs/rules/prefer-await-to-then.md",
                "prefer-await-to-then",
            ),
            (
                "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-flag.html",
                "no-useless-flag",
            ),
        ];
        for (url, expected) in cases {
            assert_eq!(eslint_rule_from_doc_url(url), Some(expected), "url {url}");
        }
    }

    #[test]
    fn doc_url_without_an_eslint_rule_page_yields_nothing() {
        // A plugin landing page names no rule; a `#rules` fragment is not a
        // path segment; another linter's rule page uses its own rule names.
        let urls = [
            "https://typescript-eslint.io/rules/",
            "https://github.com/NickvanDyke/eslint-plugin-react-perf#rules",
            "https://github.com/thepassle/eslint-plugin-barrel-files",
            "https://biomejs.dev/linter/rules/no-process-global/",
            "https://react.dev/reference/react/useEffect",
        ];
        for url in urls {
            assert_eq!(eslint_rule_from_doc_url(url), None, "url {url}");
        }
    }

    #[test]
    fn unprefixed_rule_name_drops_the_plugin_scope() {
        assert_eq!(unprefixed_rule_name("no-empty-function"), "no-empty-function");
        assert_eq!(unprefixed_rule_name("typescript/array-type"), "array-type");
        assert_eq!(
            unprefixed_rule_name("@typescript-eslint/no-explicit-any"),
            "no-explicit-any"
        );
        assert_eq!(unprefixed_rule_name("@next/next/no-img-element"), "no-img-element");
    }

    #[test]
    fn registered_rules_expose_their_upstream_eslint_name() {
        // Native rule → derived from doc_url; oxlint-delegated rule → derived
        // from the oxlint key even though it carries no doc_url.
        assert_eq!(upstream_eslint_rule("ts-no-empty-function"), Some("no-empty-function"));
        assert_eq!(upstream_eslint_rule("ts-no-explicit-any"), Some("no-explicit-any"));
        assert_eq!(
            upstream_eslint_rule("ts-no-inferrable-types"),
            Some("no-inferrable-types")
        );
        assert_eq!(upstream_eslint_rule("typescript/array-type"), Some("array-type"));
        assert_eq!(
            upstream_eslint_rule("consistent-type-imports"),
            Some("consistent-type-imports")
        );
    }

    #[test]
    fn comply_original_rule_has_no_upstream_eslint_name() {
        assert_eq!(upstream_eslint_rule("no-weak-cipher"), None);
        assert_eq!(upstream_eslint_rule("not-a-real-rule-id-zzz-xyz"), None);
    }

    #[test]
    fn doc_url_pointing_at_a_related_rule_claims_no_identity() {
        // `prefer-early-return` cites the `no-else-return` page as background,
        // and `no-else-return` is a separate check comply also registers.
        // `security-detect-insecure-randomness` covers more than the
        // `detect-pseudoRandomBytes` page it cites. Neither may inherit the
        // cited name, or a directive would suppress the wrong finding.
        assert_eq!(upstream_eslint_rule("prefer-early-return"), None);
        assert_eq!(upstream_eslint_rule("security-detect-insecure-randomness"), None);
    }

    #[test]
    fn lookup_is_memoized_across_calls() {
        // Two successive calls should hit the same OnceLock instance and
        // return equal RuleMeta (observed via id equality since RuleMeta
        // itself isn't PartialEq).
        let a = lookup("no-weak-cipher").unwrap();
        let b = lookup("no-weak-cipher").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.description, b.description);
    }
}
