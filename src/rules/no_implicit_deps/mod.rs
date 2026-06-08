//! no-implicit-deps

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
    let root = specifier.split('/').next().unwrap_or(specifier);
    NODE_BUILTINS.contains(&root) || RUNTIME_BUILTINS.contains(&root)
}

const VIRTUAL_PREFIXES: &[&str] = &[
    "virtual:",
    "@theme/",
    "@docusaurus/",
    "@internal/",
    "~react-pages",
];

pub(super) fn is_virtual_module(spec: &str) -> bool {
    VIRTUAL_PREFIXES.iter().any(|p| spec.starts_with(p))
}

pub(super) fn is_bare_specifier(spec: &str) -> bool {
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
