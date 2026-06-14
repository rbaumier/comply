//! no-implicit-deps

mod module_federation;
mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-implicit-deps",
    description: "Import of a bare specifier that is not a known Node.js builtin — may be an unlisted dependency.",
    remediation: "Ensure the package is listed in `package.json` dependencies. Bare specifier imports that are neither relative paths nor Node.js builtins may break when not explicitly installed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

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

const NODE_BUILTINS: &[&str] = &[
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
];

const RUNTIME_BUILTINS: &[&str] = &["k6", "bun", "deno"];

pub(super) fn is_node_builtin(specifier: &str) -> bool {
    if let Some(rest) = specifier.strip_prefix("node:") {
        return !rest.is_empty();
    }
    // Bun exposes runtime built-ins under the `bun:` scheme (`bun:test`,
    // `bun:sqlite`, `bun:ffi`, `bun:jsc`). They are provided by the runtime,
    // never installable from npm, so they belong in `package.json` no more
    // than `node:` builtins do.
    if let Some(rest) = specifier.strip_prefix("bun:") {
        return !rest.is_empty();
    }
    let root = specifier.split('/').next().unwrap_or(specifier);
    NODE_BUILTINS.contains(&root) || RUNTIME_BUILTINS.contains(&root)
}

const VIRTUAL_PREFIXES: &[&str] = &[
    "@theme/",
    "@theme-original/",
    "@docusaurus/",
    "@site/",
    "@internal/",
    "~react-pages",
];

/// Schemes that, despite carrying a `:`, are not virtual modules: `node:`
/// builtins and URL specifiers. These are classified by `is_node_builtin` /
/// `is_bare_specifier` before this predicate is reached, but excluding them
/// here keeps the predicate self-contained for callers that hand it a raw
/// specifier root.
const NON_VIRTUAL_SCHEMES: &[&str] = &["node", "http", "https"];

/// True if `spec` is a build-time virtual module rather than an npm package.
///
/// Two forms are recognized:
///   - a known framework virtual-namespace prefix (Docusaurus theme aliases
///     and the `@site/` project-root alias, Vite Pages), or
///   - a root package segment containing a `:`. npm package names cannot
///     contain `:`, so a colon marks a plugin-provided virtual namespace —
///     Vite's `virtual:` convention (`virtual:vitest-custom-virtual-file-1`)
///     or a custom separator (`vitest-custom-virtual:math`). `node:` builtins
///     and `http`/`https` URLs are excluded.
pub(crate) fn is_virtual_module(spec: &str) -> bool {
    if VIRTUAL_PREFIXES.iter().any(|p| spec.starts_with(p)) {
        return true;
    }
    let root = spec.split('/').next().unwrap_or(spec);
    match root.split_once(':') {
        Some((scheme, _)) => !NON_VIRTUAL_SCHEMES.contains(&scheme),
        None => false,
    }
}

/// True if `spec` is a Node.js subpath import (a `#`-prefixed internal alias
/// declared in `package.json`'s `imports` field). Node.js reserves the `#`
/// prefix exclusively for these aliases, so they are never npm package names.
pub(super) fn is_subpath_import(spec: &str) -> bool {
    spec.starts_with('#')
}

/// True if `spec` is a bare specifier — a candidate npm package name rather
/// than a relative path (`./`, `../`), an absolute path (`/`), or a URL import
/// (`http://`, `https://`). URL imports are resolved by the runtime/CDN and are
/// never npm packages, so they are excluded here.
pub(crate) fn is_bare_specifier(spec: &str) -> bool {
    !spec.starts_with('.')
        && !spec.starts_with('/')
        && !spec.starts_with("http://")
        && !spec.starts_with("https://")
}

#[cfg(test)]
pub(super) fn strip_quotes(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

/// Collapse a bare specifier to the name that would appear in `package.json`.
pub(super) fn root_package_name(spec: &str) -> &str {
    if let Some(rest) = spec.strip_prefix('@') {
        let mut parts = rest.splitn(3, '/');
        match (parts.next(), parts.next()) {
            (Some(scope), Some(name)) => {
                let len = 1 + scope.len() + 1 + name.len();
                &spec[..len]
            }
            _ => spec,
        }
    } else {
        spec.split('/').next().unwrap_or(spec)
    }
}

/// True if `spec` matches any alias prefix (exact or `prefix/...`).
pub(super) fn matches_alias(spec: &str, alias_prefixes: &[String]) -> bool {
    alias_prefixes.iter().any(|p| {
        if p.is_empty() {
            return false;
        }
        if spec == p.as_str() {
            return true;
        }
        if let Some(rest) = spec.strip_prefix(p.as_str()) {
            return rest.is_empty() || rest.starts_with('/') || p.ends_with('/');
        }
        false
    })
}
