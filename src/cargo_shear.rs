//! cargo-shear subprocess — flag unused dependencies in `Cargo.toml`.
//!
//! Why this lives in Comply: dead deps are a security and compile-time
//! liability. Each unused crate widens the supply chain, slows builds, and
//! adds version-bump churn. `cargo shear` (https://crates.io/crates/cargo-shear)
//! detects them by walking the source tree and comparing imports against
//! the `[dependencies]` table.
//!
//! How it works:
//! 1. `is_available()` probes `cargo shear --version`. Cached in a
//!    `OnceLock`.
//! 2. `lint_files()` finds the workspace root for any `.rs` file in the
//!    input set (the nearest `Cargo.toml` ancestor) and runs:
//!
//!        cargo shear --format=json <workspace>
//!
//! 3. The JSON output is parsed; each "finding" of `code = shear/unused_dependency`
//!    becomes one comply diagnostic on the offending `Cargo.toml`. The
//!    byte `offset` is converted to a 1-based line/column by reading the
//!    manifest and counting newlines.
//! 4. `drop_proven_used()` then drops the findings rustc contradicts.
//!    cargo-shear reads sources and never expands a macro.
//!    It misses code a derive emits, like `::strum::ParseError`.
//!    It misses code `include!` loads from `OUT_DIR`, like `include_proto!`.
//!    rustc judges after expansion, through clippy's run.
//!    `cargo metadata` gives each key the extern name rustc reports.
//!
//! Workspaces are de-duplicated so we don't shell out twice for the same
//! manifest when many `.rs` files share a parent crate.

use anyhow::{Context, Result};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use serde::de::IgnoredAny;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;
use wait_timeout::ChildExt;

use crate::clippy::{ExternUsage, PackageExterns, canonicalize_or_self};
use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::runner_helpers;

/// Stable comply rule id surfaced in diagnostics for unused-dependency
/// findings. Mirrors the convention used by clippy/oxlint runners.
pub const RULE_ID: &str = "rust-unused-dep";

/// Cached availability probe for `cargo shear`.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| runner_helpers::probe_binary("cargo", &["shear", "--version"]))
}

/// Run `cargo shear` on every workspace touched by `files` and return the
/// remapped diagnostics. Files outside any workspace are skipped silently —
/// shear can only operate on a real Cargo project root.
#[must_use = "diagnostics from cargo-shear must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let mut diagnostics = Vec::new();
    let mut roots = FxHashSet::default();
    for f in files {
        if let Some(root) = runner_helpers::find_cargo_workspace_root(&f.path) {
            roots.insert(root);
        }
    }
    for workspace in roots {
        diagnostics.extend(lint_workspace(&workspace)?);
    }
    Ok(diagnostics)
}

/// Run `cargo shear --format=json` from inside `workspace` and parse the
/// result. We `current_dir(workspace)` instead of passing it as a positional
/// argument because shear honors `CARGO_MANIFEST_DIR` and the cwd more
/// reliably than its `PATH` argument when invoked from a child process.
fn lint_workspace(workspace: &Path) -> Result<Vec<Diagnostic>> {
    let output = Command::new("cargo")
        .args(["shear", "--format=json"])
        .current_dir(workspace)
        .output()
        .with_context(|| format!("failed to invoke `cargo shear` in {}", workspace.display()))?;
    // Empty stdout = nothing to parse. Either shear truly found nothing
    // or it errored before producing output; either way, return cleanly
    // rather than crashing on empty JSON.
    if output.stdout.is_empty() {
        return Ok(vec![]);
    }
    let Ok(report) = serde_json::from_slice::<ShearReport>(&output.stdout) else {
        return Ok(vec![]);
    };
    convert_findings(report.findings, workspace)
}

/// Check whether the dep named in a shear message is declared as
/// `optional = true` in the manifest. Optional deps are feature-gated
/// by design — flagging them as unused is noise because cargo-shear
/// only sees the default feature set.
fn is_optional_dep(manifest_path: &Path, message: &str) -> bool {
    let Some(dep_name) = dependency_key(message) else {
        return false;
    };
    let Ok(content) = fs::read_to_string(manifest_path) else {
        return false;
    };
    let Ok(toml) = content.parse::<toml::Value>() else {
        return false;
    };
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(deps) = toml.get(section).and_then(|d| d.as_table())
            && let Some(entry) = deps.get(dep_name)
                && entry
                    .get("optional")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    return true;
                }
    }
    false
}

/// The dependency key a shear message names between backticks.
fn dependency_key(message: &str) -> Option<&str> {
    message.split('`').nth(1).filter(|name| !name.is_empty())
}

/// Drop each finding whose dependency rustc proves used.
/// Every other diagnostic passes through untouched.
/// `cargo metadata` runs once per workspace with a finding.
pub fn drop_proven_used(diagnostics: Vec<Diagnostic>, usage: &ExternUsage) -> Vec<Diagnostic> {
    let mut workspaces: FxHashMap<PathBuf, Option<Metadata>> = FxHashMap::default();
    diagnostics
        .into_iter()
        .filter(|diag| {
            if diag.rule_id != RULE_ID {
                return true;
            }
            let key = dependency_key(&diag.message);
            let root = runner_helpers::find_cargo_workspace_root(&diag.path);
            let (Some(key), Some(root)) = (key, root) else {
                return true;
            };
            let metadata = workspaces
                .entry(root)
                .or_insert_with_key(|root| read_metadata(root));
            let proven = metadata
                .as_ref()
                .and_then(|metadata| plain_extern(metadata, &diag.path, key))
                .is_some_and(|name| {
                    usage
                        .package(&diag.path)
                        .is_some_and(|package| rustc_kept(package, &name))
                });
            !proven
        })
        .collect()
}

/// How long `cargo metadata` may run.
const METADATA_TIMEOUT: Duration = Duration::from_secs(120);

/// The most `cargo metadata` output comply reads, in bytes.
/// A larger workspace keeps cargo-shear's verdicts.
const METADATA_OUTPUT_CAP: u64 = 64 * 1024 * 1024;

/// The workspace as `cargo metadata` resolves it.
/// `None` when cargo fails, overruns its deadline or its cap.
/// `--locked` leaves `Cargo.lock` untouched.
fn read_metadata(workspace: &Path) -> Option<Metadata> {
    let mut child = Command::new("cargo")
        .args(["metadata", "--format-version=1", "--locked"])
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let Some(stdout) = child.stdout.take() else {
        return give_up(child);
    };
    // The output exceeds a pipe buffer.
    // Unread, it would block cargo until the deadline.
    let reader = std::thread::spawn(move || {
        let mut raw = Vec::new();
        stdout
            .take(METADATA_OUTPUT_CAP + 1)
            .read_to_end(&mut raw)
            .map(|_| raw)
    });
    let Ok(Some(status)) = child.wait_timeout(METADATA_TIMEOUT) else {
        return give_up(child);
    };
    let raw = reader.join().ok()?.ok()?;
    let fits = u64::try_from(raw.len()).is_ok_and(|len| len <= METADATA_OUTPUT_CAP);
    if !status.success() || !fits {
        return None;
    }
    serde_json::from_slice(&raw).ok()
}

/// Stop a `cargo metadata` that outstayed its deadline.
/// A failed kill or wait means it already exited.
fn give_up(mut child: Child) -> Option<Metadata> {
    child.kill().ok();
    child.wait().ok();
    None
}

/// The extern name rustc gives `key` in `manifest`'s package.
/// `None` unless `key` is a plain dependency, unambiguously named.
fn plain_extern(metadata: &Metadata, manifest: &Path, key: &str) -> Option<String> {
    let manifest = canonicalize_or_self(manifest);
    let package = metadata
        .packages
        .iter()
        .find(|package| canonicalize_or_self(&package.manifest_path) == manifest)?;
    let entries: Vec<&MetadataDependency> = package
        .dependencies
        .iter()
        .filter(|dep| dep.rename.as_deref().unwrap_or(&dep.name) == key)
        .collect();
    let first = entries.first()?;
    // A dev, build or platform entry misses some targets.
    // Their silence would then prove nothing.
    if entries
        .iter()
        .any(|dep| dep.kind.is_some() || dep.target.is_some() || dep.optional)
    {
        return None;
    }
    if let Some(rename) = &first.rename {
        return Some(rename.replace('-', "_"));
    }
    // Unrenamed, rustc uses the library's own name.
    // `async-stripe-billing` builds `stripe_billing`, for one.
    let names: FxHashMap<&str, &str> = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package.name.as_str()))
        .collect();
    let node = metadata
        .resolve
        .as_ref()?
        .nodes
        .iter()
        .find(|node| node.id == package.id)?;
    let mut externs = node
        .deps
        .iter()
        .filter(|dep| names.get(dep.pkg.as_str()) == Some(&first.name.as_str()))
        .map(|dep| dep.name.as_str());
    let name = externs.next()?;
    // Two names for one package leave the proof ambiguous.
    externs.all(|other| other == name).then(|| name.to_owned())
}

/// Whether one compiled target kept the extern `name`.
/// An unreadable unused-extern warning proves nothing.
fn rustc_kept(package: &PackageExterns, name: &str) -> bool {
    !package.unreadable
        && package.compiled.iter().any(|target| {
            !package
                .unused
                .get(target)
                .is_some_and(|unused| unused.contains(name))
        })
}

/// Convert each shear finding into a comply Diagnostic. We only emit
/// diagnostics for `shear/unused_dependency` findings — shear also surfaces
/// `shear/unlinked_files` (orphan source files) but those are out of scope
/// for a "no unused deps" rule and would muddy the signal. Each unused-dep
/// finding includes a `file` and a byte `offset`; we translate the offset
/// to a 1-based line/column for editor compatibility.
fn convert_findings(findings: Vec<Finding>, workspace: &Path) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    for finding in findings {
        if finding.code != "shear/unused_dependency" {
            continue;
        }
        let Some(file) = finding.file else { continue };
        let manifest_path = workspace.join(&file);
        if is_optional_dep(&manifest_path, &finding.message) {
            continue;
        }
        let offset = finding.location.map(|l| l.offset).unwrap_or(0);
        let (line, column) = byte_offset_to_line_col(&manifest_path, offset).unwrap_or((1, 1));
        diagnostics.push(Diagnostic {
            path: manifest_path.into(),
            line,
            column,
            rule_id: RULE_ID.into(),
            message: format!(
                "{} — every unused dep widens the supply chain, slows builds, \
                 and adds version-bump churn. Remove it from `[dependencies]` \
                 (or run `cargo shear --fix`).",
                finding.message
            ),
            severity: Severity::Error,
            span: None,
        });
    }
    Ok(diagnostics)
}

/// Read `path` and convert a byte `offset_bytes` into a 1-based
/// (line, column). Falls back to (1, 1) if the file can't be read —
/// better to surface the finding at the top of the manifest than to
/// drop it entirely.
fn byte_offset_to_line_col(path: &Path, offset_bytes: usize) -> Option<(usize, usize)> {
    let content = fs::read_to_string(path).ok()?;
    let bytes = content.as_bytes();
    let clamped = offset_bytes.min(bytes.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for &b in &bytes[..clamped] {
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    Some((line, col))
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct ShearReport {
    #[serde(default)]
    findings: Vec<Finding>,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct Finding {
    code: String,
    message: String,
    /// Optional — `shear/unlinked_files` findings have no file field.
    /// We skip those upstream so the absence is fine.
    #[serde(default)]
    file: Option<String>,
    /// Optional for the same reason.
    #[serde(default)]
    location: Option<Location>,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
struct Location {
    offset: usize,
    #[allow(dead_code)]
    length: usize,
}

/// What comply reads of `cargo metadata --format-version=1`.
#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<MetadataPackage>,
    /// Absent under `--no-deps`.
    #[serde(default)]
    resolve: Option<MetadataResolve>,
    #[serde(default, flatten)]
    _extra: IgnoredAny,
}

/// One package of the metadata.
#[derive(Debug, Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    manifest_path: PathBuf,
    dependencies: Vec<MetadataDependency>,
    #[serde(default, flatten)]
    _extra: IgnoredAny,
}

/// One manifest entry, after workspace inheritance.
#[derive(Debug, Deserialize)]
struct MetadataDependency {
    /// The package name, not the key.
    name: String,
    /// The key, when it differs from the package name.
    #[serde(default)]
    rename: Option<String>,
    /// `None` for `[dependencies]`, else `dev` or `build`.
    #[serde(default)]
    kind: Option<String>,
    /// The `cfg(…)` of a platform-specific entry.
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    optional: bool,
    #[serde(default, flatten)]
    _extra: IgnoredAny,
}

/// The resolved dependency graph.
#[derive(Debug, Deserialize)]
struct MetadataResolve {
    nodes: Vec<MetadataNode>,
    #[serde(default, flatten)]
    _extra: IgnoredAny,
}

/// One package's resolved edges.
#[derive(Debug, Deserialize)]
struct MetadataNode {
    id: String,
    deps: Vec<MetadataEdge>,
    #[serde(default, flatten)]
    _extra: IgnoredAny,
}

/// One resolved edge.
#[derive(Debug, Deserialize)]
struct MetadataEdge {
    /// The extern name rustc sees.
    name: String,
    pkg: String,
    #[serde(default, flatten)]
    _extra: IgnoredAny,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_finding_json() {
        let raw = br#"{"summary":{"errors":1,"warnings":0,"fixed":0},"findings":[{"code":"shear/unused_dependency","severity":"error","message":"unused dependency `serde`","file":"Cargo.toml","location":{"offset":75,"length":5},"help":"remove this dependency","fixable":true}]}"#;
        let report: ShearReport = serde_json::from_slice(raw).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].message, "unused dependency `serde`");
    }

    #[test]
    fn empty_findings_array() {
        let raw = br#"{"summary":{"errors":0,"warnings":0,"fixed":0},"findings":[]}"#;
        let report: ShearReport = serde_json::from_slice(raw).unwrap();
        assert!(report.findings.is_empty());
    }

    /// Metadata for package `app`, with the given entries and edges.
    fn app_metadata(dependencies: &str, edges: &str, packages: &str) -> Metadata {
        serde_json::from_str(&format!(
            r#"{{"packages":[{{"id":"app","name":"app","manifest_path":"/ws/app/Cargo.toml","dependencies":[{dependencies}]}}{packages}],
               "resolve":{{"nodes":[{{"id":"app","deps":[{edges}]}}]}}}}"#
        ))
        .unwrap()
    }

    /// A package `name` with id `id` and no dependencies.
    fn package(id: &str, name: &str) -> String {
        format!(r#",{{"id":"{id}","name":"{name}","manifest_path":"/reg/{id}","dependencies":[]}}"#)
    }

    #[test]
    fn plain_extern_names_the_library_rustc_reports() {
        let metadata = app_metadata(
            r#"{"name":"async-stripe-billing"},{"name":"wire-codec","rename":"wire-renamed"}"#,
            r#"{"name":"stripe_billing","pkg":"billing"},{"name":"wire_renamed","pkg":"wire"}"#,
            &(package("billing", "async-stripe-billing") + &package("wire", "wire-codec")),
        );
        let manifest = Path::new("/ws/app/Cargo.toml");
        assert_eq!(
            plain_extern(&metadata, manifest, "async-stripe-billing").as_deref(),
            Some("stripe_billing")
        );
        assert_eq!(
            plain_extern(&metadata, manifest, "wire-renamed").as_deref(),
            Some("wire_renamed")
        );
        assert_eq!(plain_extern(&metadata, manifest, "wire-codec"), None);
        assert_eq!(
            plain_extern(&metadata, Path::new("/ws/other/Cargo.toml"), "wire-renamed"),
            None
        );
    }

    #[test]
    fn only_a_plain_unambiguous_dependency_has_an_extern() {
        let metadata = app_metadata(
            r#"{"name":"both"},{"name":"both","kind":"dev"},{"name":"build","kind":"build"},
               {"name":"unix","target":"cfg(unix)"},{"name":"opt","optional":true},
               {"name":"twice"},{"name":"twice","rename":"twice-v2"}"#,
            r#"{"name":"twice","pkg":"t1"},{"name":"twice_v2","pkg":"t2"}"#,
            &(package("t1", "twice") + &package("t2", "twice")),
        );
        // A target without the extern stays silent: no proof.
        for key in ["both", "build", "unix", "opt", "twice", "absent"] {
            assert_eq!(
                plain_extern(&metadata, Path::new("/ws/app/Cargo.toml"), key),
                None,
                "{key}"
            );
        }
    }

    /// One compiled `lib` target, with the externs rustc found unused.
    fn lib_leaving_unused(names: &[&str]) -> PackageExterns {
        let lib = (vec!["lib".to_owned()], "app".to_owned());
        let mut package = PackageExterns::default();
        package.compiled.insert(lib.clone());
        package
            .unused
            .insert(lib, names.iter().map(|n| (*n).to_owned()).collect());
        package
    }

    #[test]
    fn a_target_where_rustc_kept_the_extern_proves_it_used() {
        let mut package = lib_leaving_unused(&["idle"]);
        assert!(rustc_kept(&package, "tonic_prost"));
        assert!(!rustc_kept(&package, "idle"));
        // A test target leaving `tonic_prost` unused changes nothing.
        let test = (vec!["test".to_owned()], "smoke".to_owned());
        package.compiled.insert(test.clone());
        package
            .unused
            .insert(test, ["tonic_prost".to_owned()].into_iter().collect());
        assert!(rustc_kept(&package, "tonic_prost"));
    }

    #[test]
    fn an_unreadable_warning_or_no_compiled_target_proves_nothing() {
        let mut unreadable = lib_leaving_unused(&[]);
        unreadable.unreadable = true;
        assert!(!rustc_kept(&unreadable, "prost"));
        assert!(!rustc_kept(&PackageExterns::default(), "prost"));
    }

    #[test]
    fn byte_offset_at_start() {
        let tmp = std::env::temp_dir().join("comply-shear-test.toml");
        fs::write(&tmp, "line1\nline2\nline3").unwrap();
        assert_eq!(byte_offset_to_line_col(&tmp, 0), Some((1, 1)));
        assert_eq!(byte_offset_to_line_col(&tmp, 6), Some((2, 1)));
        assert_eq!(byte_offset_to_line_col(&tmp, 12), Some((3, 1)));
        let _ = fs::remove_file(&tmp);
    }
}
