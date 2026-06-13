//! no-unsupported-node-builtins — flag use of Node.js APIs that aren't
//! available in the minimum Node version declared in `engines.node`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

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

/// One global API: name, minimum Node major version it became available at, and
/// whether it is a browser DOM global (a member of `lib.dom.d.ts` that exists
/// natively in browsers and was only later added to Node's global scope).
///
/// The WHATWG web-platform value types (`Request`, `Response`, `Headers`,
/// `FormData`, `Blob`, `File`) are intentionally NOT version-gated: they are
/// `lib.dom.d.ts` standards available across all modern runtimes (browsers,
/// Deno, Bun, Cloudflare Workers, Node 18+) and the core abstraction of
/// multi-runtime frameworks, so gating them by `engines.node` produces false
/// positives. `fetch` itself (the I/O function) stays gated.
///
/// Browser-DOM globals (`browser_dom = true`) are additionally exempt when the
/// project targets the browser (see [`targets_browser`]): in a browser bundle
/// `engines.node` constrains the build toolchain, not the runtime, so flagging
/// `CustomEvent`/`navigator` there is a false positive.
struct GlobalApi {
    name: &'static str,
    min_version: u32,
    browser_dom: bool,
}

const GLOBAL_APIS: &[GlobalApi] = &[
    GlobalApi { name: "AbortController", min_version: 15, browser_dom: false },
    GlobalApi { name: "AbortSignal", min_version: 15, browser_dom: false },
    GlobalApi { name: "BroadcastChannel", min_version: 15, browser_dom: true },
    GlobalApi { name: "atob", min_version: 16, browser_dom: false },
    GlobalApi { name: "btoa", min_version: 16, browser_dom: false },
    GlobalApi { name: "structuredClone", min_version: 17, browser_dom: false },
    GlobalApi { name: "fetch", min_version: 18, browser_dom: false },
    GlobalApi { name: "CustomEvent", min_version: 19, browser_dom: true },
    GlobalApi { name: "navigator", min_version: 21, browser_dom: true },
    GlobalApi { name: "WebSocket", min_version: 22, browser_dom: true },
];

/// Dependencies whose presence means the package's source compiles to a browser
/// bundle (web-component compilers / browser-only UI runtimes). For such a
/// package `engines.node` describes the build toolchain, not where the shipped
/// code runs, so browser DOM globals are legitimate.
const BROWSER_FRAMEWORK_DEPS: &[&str] = &["@stencil/core", "lit", "lit-element", "lit-html"];

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

pub(super) fn lookup_global(name: &str) -> Option<u32> {
    GLOBAL_APIS
        .iter()
        .find(|api| api.name == name)
        .map(|api| api.min_version)
}

/// True if `name` is a browser DOM global (exists natively in browsers, added to
/// Node later). These are exempt when the file targets the browser.
pub(super) fn is_browser_dom_global(name: &str) -> bool {
    GLOBAL_APIS
        .iter()
        .any(|api| api.name == name && api.browser_dom)
}

/// True if the file at `path` ships in a browser bundle, so its `engines.node`
/// constraint applies to build tooling rather than the runtime.
///
/// Signals (any one suffices): the nearest `package.json` declares a browser
/// runtime target (`browserslist`, `engines.electron`, `engines.vscode`) or
/// depends on a browser-bundling UI framework (see [`BROWSER_FRAMEWORK_DEPS`]).
pub(super) fn targets_browser(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return false;
    };
    pkg.has_browserslist
        || pkg.engines.contains_key("electron")
        || pkg.engines.contains_key("vscode")
        || BROWSER_FRAMEWORK_DEPS
            .iter()
            .any(|dep| pkg.has_dep_or_engine(dep))
}

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
