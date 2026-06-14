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

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

/// Genuine Node.js core synchronous I/O methods that block the event loop.
///
/// Restricted to the `fs` / `fs/promises` and `child_process` `*Sync` methods —
/// the only Node core APIs whose synchronous form performs blocking I/O. Any
/// other identifier ending in `Sync` (framework APIs like Svelte's `flushSync`,
/// React's `flushSync`, application helpers like `batchSync`) is not Node I/O
/// and must not be flagged.
const NODE_SYNC_IO_METHODS: &[&str] = &[
    // child_process
    "execSync",
    "execFileSync",
    "spawnSync",
    // fs / fs.promises
    "accessSync",
    "appendFileSync",
    "chmodSync",
    "chownSync",
    "closeSync",
    "copyFileSync",
    "cpSync",
    "existsSync",
    "fchmodSync",
    "fchownSync",
    "fdatasyncSync",
    "fstatSync",
    "fsyncSync",
    "ftruncateSync",
    "futimesSync",
    "globSync",
    "lchmodSync",
    "lchownSync",
    "linkSync",
    "lstatSync",
    "lutimesSync",
    "mkdirSync",
    "mkdtempSync",
    "opendirSync",
    "openSync",
    "readFileSync",
    "readSync",
    "readdirSync",
    "readlinkSync",
    "readvSync",
    "realpathSync",
    "renameSync",
    "rmSync",
    "rmdirSync",
    "statSync",
    "statfsSync",
    "symlinkSync",
    "truncateSync",
    "unlinkSync",
    "utimesSync",
    "writeFileSync",
    "writeSync",
    "writevSync",
];

/// Returns true when the method name is a genuine Node.js core synchronous I/O
/// call (e.g. `readFileSync`, `execSync`) — as opposed to any unrelated
/// identifier that merely ends in `Sync`.
pub(super) fn is_node_sync_io_method(method_name: &str) -> bool {
    NODE_SYNC_IO_METHODS.contains(&method_name)
}

/// Returns true when an enclosing function's name advertises a synchronous
/// contract (suffix `Sync`, e.g. `copyDirSync`, `walkSync`). Inside such a
/// function, synchronous I/O is the intended behaviour, mirroring Node's own
/// naming convention for synchronous variants.
pub(super) fn function_name_is_sync(name: &str) -> bool {
    name.ends_with("Sync")
}

pub(super) fn allows_sync_node_api(path: &std::path::Path, source: &str, in_cli_package: bool) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    in_cli_package
        || lower.starts_with("scripts/")
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
