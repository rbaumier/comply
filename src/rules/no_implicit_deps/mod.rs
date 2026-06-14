//! no-implicit-deps

mod jest_module_roots;
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
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
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
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
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
    // `~icons/` is the virtual-module namespace injected by `unplugin-icons`
    // (Vite/Webpack/Nuxt); each `~icons/<collection>/<name>` resolves to a
    // generated icon component at build time, never an npm package.
    "~icons/",
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

/// Build-time virtual module specifiers injected by SvelteKit's official
/// adapters. The adapter's Rollup/Vite plugin resolves each of these bare
/// uppercase specifiers to generated code at bundle time (`HANDLER` → the
/// request handler, `SERVER` → the SSR server, `MANIFEST` → the route
/// manifest, `ENV` → the env accessor, `SHIMS` → runtime shims). They are
/// intentionally absent from `package.json` — they are never npm packages.
const SVELTEKIT_ADAPTER_VIRTUAL_MODULES: &[&str] =
    &["HANDLER", "ENV", "SERVER", "SHIMS", "MANIFEST"];

/// True if `spec` is a SvelteKit adapter build-time virtual module name.
/// Gated by the caller on SvelteKit detection so the same uppercase specifier
/// remains a genuine implicit-dependency error in a non-SvelteKit project.
pub(super) fn is_sveltekit_adapter_virtual_module(spec: &str) -> bool {
    SVELTEKIT_ADAPTER_VIRTUAL_MODULES.contains(&spec)
}

/// SvelteKit's reserved `$`-prefixed application aliases. These are resolved by
/// SvelteKit's Vite plugin to project source or generated code, never to an npm
/// package: `$lib` maps to `src/lib`, `$app/*` exposes the runtime modules
/// (`$app/navigation`, `$app/stores`, `$app/environment`, …), `$env/*` exposes
/// the typed env accessors (`$env/static/private`, `$env/dynamic/public`, …),
/// and `$service-worker` exposes the service-worker module. They are reserved by
/// the framework and not user-configurable, so they never appear in
/// `package.json`.
const SVELTEKIT_APP_ALIAS_PREFIXES: &[&str] = &["$app/", "$env/", "$lib/"];

/// True if `spec` is a SvelteKit reserved application alias (`$lib`, `$lib/…`,
/// `$app/…`, `$env/…`, or `$service-worker`). Gated by the caller on SvelteKit
/// detection so a `$`-prefixed specifier remains a genuine implicit-dependency
/// error in a non-SvelteKit project. Only the documented aliases match — an
/// arbitrary `$`-prefixed specifier is not exempted.
pub(super) fn is_sveltekit_app_alias(spec: &str) -> bool {
    spec == "$lib"
        || spec == "$service-worker"
        || SVELTEKIT_APP_ALIAS_PREFIXES.iter().any(|p| spec.starts_with(p))
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

/// DefinitelyTyped package name that provides the type declarations for a
/// runtime package: `json-schema` → `@types/json-schema`, and a scoped
/// `@foo/bar` → `@types/foo__bar` (TypeScript folds the scope separator to a
/// double underscore). A project that lists only `@types/X` in its
/// `(dev|peer)dependencies` can still `import … from "X"`, since TypeScript
/// resolves the bare specifier to those declarations.
pub(crate) fn types_package_name(spec: &str) -> String {
    if let Some(scoped) = spec.strip_prefix('@') {
        return format!("@types/{}", scoped.replacen('/', "__", 1));
    }
    format!("@types/{spec}")
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
