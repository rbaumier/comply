//! node-no-sync

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-sync",
    description: "Synchronous Node.js methods block the event loop.",
    remediation: "Use the asynchronous variant (e.g. `readFile` instead of `readFileSync`) or `fs.promises`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub(super) fn allows_sync_node_api(path: &std::path::Path, source: &str) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    lower.starts_with("scripts/")
        || lower.contains("/scripts/")
        || lower.starts_with("bin/")
        || lower.contains("/bin/")
        || lower.starts_with("tools/")
        || lower.contains("/tools/")
        || lower.starts_with("cli/")
        || lower.contains("/cli/")
        || file_name_is_config(path)
        || source
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("#!") && line.contains("node"))
}

fn file_name_is_config(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| file_name.contains(".config."))
}

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
