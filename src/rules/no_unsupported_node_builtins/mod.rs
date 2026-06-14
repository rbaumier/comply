//! no-unsupported-node-builtins — flag use of newer ECMAScript built-in
//! methods that aren't available in the minimum Node version declared in
//! `engines.node` (e.g. `Array.prototype.findLast`, `Object.hasOwn`,
//! `Array.fromAsync`).
//!
//! Web-platform globals (`fetch`, `Request`, `Response`, `WebSocket`,
//! `AbortController`, `structuredClone`, `atob`/`btoa`, `CustomEvent`,
//! `navigator`, …) are WHATWG / WinterCG standards available across every
//! modern runtime (browsers, Deno, Bun, Cloudflare Workers, Node 18+), so they
//! are runtime-agnostic and never version-gated by this rule.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsupported-node-builtins",
    description: "Node.js API not available in the minimum version declared in `engines.node`.",
    remediation: "Either bump the minimum Node.js version in `engines.node`, or use a polyfill.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-unsupported-features/node-builtins.md",
    ),
    categories: &["node"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}

// ── shared helpers (used by both tree-sitter and oxc backends) ──────

/// Instance methods introduced on Array.prototype / typed array prototypes.
const INSTANCE_METHODS: &[(&str, u32)] = &[
    ("findLast", 18),
    ("findLastIndex", 18),
    ("toSorted", 20),
    ("toReversed", 20),
    ("toSpliced", 20),
    ("with", 20),
    ("groupBy", 21),
];

/// Directory path segments that unambiguously indicate a non-Node.js runtime.
/// Files under these paths are exempt from Node.js API version checks.
pub(super) const NON_NODE_RUNTIME_DIRS: &[&str] = &["deno/", "cloudflare-workers/"];

/// Static methods on well-known constructors.
const STATIC_METHODS: &[(&str, &str, u32)] = &[
    ("Object", "hasOwn", 16),
    ("Object", "groupBy", 21),
    ("Array", "fromAsync", 22),
];

pub(super) fn lookup_instance_method(name: &str) -> Option<u32> {
    INSTANCE_METHODS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
}

pub(super) fn lookup_static_method(obj: &str, prop: &str) -> Option<u32> {
    STATIC_METHODS
        .iter()
        .find(|(o, p, _)| *o == obj && *p == prop)
        .map(|(_, _, v)| *v)
}

/// Parse the minimum Node major version from an `engines.node` range string.
pub(super) fn parse_min_version(spec: &str) -> Option<u32> {
    let mut minimum: Option<u32> = None;
    for alt in spec.split("||") {
        if let Some(v) = parse_range_min(alt) {
            minimum = Some(minimum.map_or(v, |m| m.min(v)));
        }
    }
    minimum
}

fn parse_range_min(range: &str) -> Option<u32> {
    let bytes = range.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            return std::str::from_utf8(&bytes[start..i]).ok()?.parse().ok();
        }
        i += 1;
    }
    None
}

pub(super) fn min_node_major(ctx: &crate::rules::backend::CheckCtx) -> Option<u32> {
    let pkg = ctx.project.nearest_package_json(ctx.path)?;
    let spec = pkg.engines.get("node")?;
    parse_min_version(spec)
}
