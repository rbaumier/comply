//! Type-aware analysis phase.
//!
//! Runs comply's custom type-aware rules (`Backend::TypeAware`) that need a
//! resolved TypeScript program — type queries, structural comparison — which
//! no in-process tree-sitter pass can answer. comply drives a TypeScript
//! checker (typescript-go, from `typescript` >= 7, its side-by-side alias
//! `@typescript/native`, or else `@typescript/native-preview`) through a Node sidecar: it sends
//! the tsconfig + file list + enabled rule ids, the sidecar builds the program once and
//! returns the violations as JSON.
//!
//! The checker is resolved from the project first.
//! A machine-wide install answers next, in three layouts.
//! npm's global root, then volta's images, then `tsgo`'s own tree.
//! `$COMPLY_TYPESCRIPT_GO_ROOTS` names those directories instead.
//! A repo on TypeScript 5 is not a missing toolchain.
//! Refusing it earns each repo a symlink.
//!
//! Invoked when at least one rule needing that program is still enabled for
//! the project (`main::type_aware_rules_active`). Building the program
//! dominates the run, so a project that disables them all skips the phase
//! entirely and keeps the AstCheck-only budget.
//!
//! Failure handling splits by cause, because silently skipping the phase makes
//! a clean exit indistinguishable from "the toolchain wasn't there". A
//! *toolchain* failure — `node` absent, none of `typescript` >= 7,
//! `@typescript/native` or `@typescript/native-preview` resolvable anywhere, the checker API failing to
//! initialize, or the program snapshot failing — is a hard `ToolchainError`:
//! non-zero exit, and the remedy comply knows. A missing tsconfig (nothing to type-check) and a sidecar timeout (a
//! hang, not a toolchain gap) stay graceful: a one-line stderr notice and no
//! diagnostics.
//!
//! A file no TypeScript program holds is refused.
//! Skipping it would pass code nothing type-checked.

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use wait_timeout::ChildExt;

use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::files::SourceFile;
use crate::toolchain::ToolchainError;

/// The sidecar is embedded so the binary is self-contained; it's written to a
/// temp file per run and executed with `node`.
const SIDECAR_SRC: &str = include_str!("sidecar.mjs");

/// Generous ceiling — type-aware analysis builds the full TS program, which is
/// the expensive part. Anything past this is treated as a hang, not a result.
const SIDECAR_TIMEOUT: Duration = Duration::from_secs(300);

/// The one sidecar `error` comply can still act on.
/// Nothing it was asked to resolve was there.
const PACKAGE_NOT_FOUND: &str = "package-not-found";

/// The packages that ship the typescript-go API.
/// Ordered as the sidecar prefers them.
/// Mirrored here: volta keys one image per package name.
const TYPESCRIPT_GO_PACKAGES: [&str; 3] = [
    "typescript",
    "@typescript/native",
    "@typescript/native-preview",
];

/// How long `npm root -g` gets to answer.
/// npm's startup dominates, and a loaded machine is slow.
/// A package manager that hangs must not hold the run.
const NPM_ROOT_TIMEOUT: Duration = Duration::from_secs(10);

/// Cap on what `npm root -g` may say.
/// The answer is one path on one line.
/// A chattier npm is read up to here, no further.
const NPM_ROOT_OUTPUT_CAP: u64 = 4096;

/// Names the `node_modules` directories to search, OS path separator.
/// It replaces the discovered ones.
/// An unknown layout is then one variable away.
/// An empty value keeps resolution project-local.
const TYPESCRIPT_GO_ROOTS_ENV: &str = "COMPLY_TYPESCRIPT_GO_ROOTS";

#[derive(Deserialize)]
struct SidecarResponse {
    diagnostics: Vec<SidecarDiag>,
    #[serde(default)]
    error: Option<String>,
    /// The files no program held, when `error` is [`UNPLACED`].
    #[serde(default)]
    files: Vec<String>,
}

/// The sidecar `error` for files it placed in no program.
const UNPLACED: &str = "unplaced";

/// How many such files a refusal names.
const QUOTED_UNPLACED: usize = 20;

#[derive(Deserialize)]
struct SidecarDiag {
    file: String,
    line: usize,
    column: usize,
    rule: String,
    message: String,
}

/// Run the enabled custom type-aware rules over `files`.
#[must_use = "diagnostics from the type-aware sidecar must be reported"]
pub fn lint_files(files: &[&SourceFile], config: &Config) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    // A rule runs when it's enabled for at least one file; per-file / per-glob
    // disables are applied later by `apply_config_filters`.
    let metas = crate::rules::collect_type_aware_bindings();
    let enabled: Vec<&'static crate::rules::meta::RuleMeta> = metas
        .into_iter()
        .filter(|m| files.iter().any(|f| config.is_rule_enabled(m.id, &f.path)))
        .collect();
    if enabled.is_empty() {
        return Ok(vec![]);
    }

    if !node_available() {
        return Err(ToolchainError::Unusable {
            what: "type-aware analysis needs Node.js on PATH to run the type-aware sidecar, but \
                   `node` was not found — install Node.js"
                .to_owned(),
        }
        .into());
    }

    let Some(tsconfig) = find_tsconfig(files) else {
        eprintln!("comply: no tsconfig.json found — skipping type-aware rules.");
        return Ok(vec![]);
    };
    let project_dir = tsconfig
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // The sidecar identifies files by their canonical path; keep a map back to
    // the path comply discovered so reported diagnostics match the rest of the
    // run (config filtering, diff-only normalization all key on that path).
    let mut canon_to_orig: rustc_hash::FxHashMap<String, PathBuf> =
        rustc_hash::FxHashMap::default();
    let mut abs_files: Vec<String> = Vec::with_capacity(files.len());
    for f in files {
        if let Ok(canon) = std::fs::canonicalize(&f.path) {
            let canon = canon.to_string_lossy().into_owned();
            canon_to_orig.insert(canon.clone(), f.path.clone());
            abs_files.push(canon);
        }
    }
    let rule_ids: Vec<&str> = enabled.iter().map(|m| m.id).collect();
    // Thresholds the sidecar needs, resolved from the authoritative TOML so the
    // single source of truth stays Rust-side. These rules are TS-family, so the
    // language-keyed lookup uses TypeScript.
    let min_dup_properties = config.threshold(
        "no-duplicate-type-definition",
        "min_properties",
        crate::files::Language::TypeScript,
    );
    let build_request = |global_roots: &[PathBuf]| {
        let roots: Vec<String> = global_roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        serde_json::json!({
            "tsconfig": tsconfig.to_string_lossy(),
            "files": abs_files,
            "rules": rule_ids,
            "config": { "no-duplicate-type-definition": { "min_properties": min_dup_properties } },
            "globalRoots": roots,
        })
        .to_string()
    };

    let Some(mut response) = run_sidecar(&project_dir, &build_request(&[]))? else {
        return Ok(vec![]);
    };

    // The project is asked alone first.
    // Resolution fails before any program is built.
    // So the fallback only costs a second sidecar start.
    if response.error.as_deref() == Some(PACKAGE_NOT_FOUND) {
        let global_roots = global_module_roots();
        if !global_roots.is_empty() {
            let Some(retried) = run_sidecar(&project_dir, &build_request(&global_roots))? else {
                return Ok(vec![]);
            };
            response = retried;
        }
        if response.error.as_deref() == Some(PACKAGE_NOT_FOUND) {
            return Err(package_not_found(&global_roots).into());
        }
    }

    match response.error.as_deref() {
        Some(UNPLACED) => return Err(unplaced_error(&response.files, &canon_to_orig).into()),
        Some(err) => return Err(sidecar_error(err).into()),
        None => {}
    }

    Ok(map_diagnostics(
        response.diagnostics,
        &enabled,
        &canon_to_orig,
    ))
}

/// Whether `node` is on PATH. Cached for the process lifetime.
fn node_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Where a machine-wide typescript-go can sit, in search order.
/// Three layouts, because none of them sees the others.
/// The refusal names the result of this search.
fn global_module_roots() -> Vec<PathBuf> {
    if let Some(declared) = std::env::var_os(TYPESCRIPT_GO_ROOTS_ENV) {
        return existing_unique(std::env::split_paths(&declared));
    }
    // nvm, fnm and pnpm have no fixed path.
    // Their `tsgo` tells.
    let volta = volta_home().map_or_else(Vec::new, |home| volta_module_roots(&home));
    let tsgo = std::env::var_os("PATH").and_then(|path| tsgo_module_root(&path));
    existing_unique(npm_global_root().into_iter().chain(volta).chain(tsgo))
}

/// The candidates that exist, in order, each listed once.
/// A global npm install can be found twice.
/// An absent directory is dropped, never named.
fn existing_unique(candidates: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for dir in candidates {
        if dir.is_dir() && !roots.contains(&dir) {
            roots.push(dir);
        }
    }
    roots
}

/// npm's own global `node_modules`.
/// `None` when npm is absent, fails, or is too slow.
/// Only npm knows its prefix, so npm is asked.
fn npm_global_root() -> Option<PathBuf> {
    let mut child = Command::new("npm")
        .args(["root", "-g"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let Ok(Some(status)) = child.wait_timeout(NPM_ROOT_TIMEOUT) else {
        return give_up(child);
    };
    if !status.success() {
        return give_up(child);
    }
    let mut stdout = String::new();
    child
        .stdout
        .take()?
        .take(NPM_ROOT_OUTPUT_CAP)
        .read_to_string(&mut stdout)
        .ok()?;
    let root = stdout.lines().next()?.trim();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

/// Stop a child that outstayed its deadline.
/// A failed kill or wait means the OS was first.
/// That is the same outcome, so neither is recoverable.
fn give_up(mut child: std::process::Child) -> Option<PathBuf> {
    child.kill().ok();
    child.wait().ok();
    None
}

/// Volta's home: `$VOLTA_HOME`, else `~/.volta`.
/// Read the way volta's own tools read it.
fn volta_home() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("VOLTA_HOME") {
        return Some(PathBuf::from(home));
    }
    Some(PathBuf::from(std::env::var_os("HOME")?).join(".volta"))
}

/// Volta's candidate roots under `volta_home`.
/// It installs each global package in its own image.
/// `npm root -g` never lists them.
/// So each name is a candidate.
fn volta_module_roots(volta_home: &Path) -> Vec<PathBuf> {
    let packages = volta_home.join("tools").join("image").join("packages");
    TYPESCRIPT_GO_PACKAGES
        .iter()
        .map(|pkg| packages.join(pkg).join("lib").join("node_modules"))
        .collect()
}

/// The `node_modules` a `tsgo` in `path` was installed into.
/// A global bin is a symlink into its own package.
/// Volta's shim is not, so it yields `None`.
fn tsgo_module_root(path: &OsStr) -> Option<PathBuf> {
    let binary = std::env::split_paths(path)
        .map(|dir| dir.join("tsgo"))
        .find(|candidate| candidate.is_file())?;
    binary
        .canonicalize()
        .ok()?
        .ancestors()
        .find(|dir| dir.file_name() == Some(OsStr::new("node_modules")))
        .map(Path::to_path_buf)
}

/// The nearest `tsconfig.json` above the first file that has one.
/// The sidecar places the other files itself.
/// `walk_up_finding` yields the directory, so the filename is appended.
fn find_tsconfig(files: &[&SourceFile]) -> Option<PathBuf> {
    files.iter().find_map(|file| {
        let start = std::fs::canonicalize(&file.path)
            .ok()?
            .parent()?
            .to_path_buf();
        let dir = crate::project::walk_up_finding(&start, "tsconfig.json")?;
        Some(dir.join("tsconfig.json"))
    })
}

/// Write the sidecar to a temp file, run it with the request on stdin, and
/// parse its JSON response. Returns `None` (with a stderr notice) on spawn
/// failure or timeout.
fn run_sidecar(project_dir: &Path, request: &str) -> Result<Option<SidecarResponse>> {
    let sidecar_path =
        std::env::temp_dir().join(format!("comply-typeaware-{}.mjs", std::process::id()));
    std::fs::write(&sidecar_path, SIDECAR_SRC)
        .with_context(|| format!("failed to write sidecar to {}", sidecar_path.display()))?;
    let _cleanup = TempFile(&sidecar_path);

    let mut child = Command::new("node")
        .arg(&sidecar_path)
        .current_dir(project_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn node for the type-aware sidecar")?;

    // A response can outgrow the pipe buffer.
    // Draining it only after the wait deadlocks.
    let mut out_pipe = child.stdout.take().context("sidecar stdout unavailable")?;
    let mut err_pipe = child.stderr.take().context("sidecar stderr unavailable")?;
    let out_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        out_pipe.read_to_string(&mut buf).map(|_| buf)
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = err_pipe.read_to_string(&mut buf);
        buf
    });

    child
        .stdin
        .take()
        .context("sidecar stdin unavailable")?
        .write_all(request.as_bytes())
        .context("failed to send request to the type-aware sidecar")?;

    let Some(_status) = child
        .wait_timeout(SIDECAR_TIMEOUT)
        .context("failed to wait for the type-aware sidecar")?
    else {
        let _ = child.kill();
        let _ = child.wait();
        eprintln!(
            "comply: type-aware sidecar timed out after {}s — skipping type-aware rules.",
            SIDECAR_TIMEOUT.as_secs()
        );
        return Ok(None);
    };

    let stdout = out_reader
        .join()
        .map_err(|_| anyhow::anyhow!("the type-aware sidecar output reader panicked"))?
        .context("failed to read sidecar output")?;

    let response: SidecarResponse = serde_json::from_str(&stdout).with_context(|| {
        let stderr = err_reader.join().unwrap_or_default();
        format!("failed to parse type-aware sidecar output. sidecar stderr: {stderr}")
    })?;
    Ok(Some(response))
}

/// The hard error for a sidecar-reported toolchain failure.
/// It covers `api-init-failed: …` and `snapshot-failed: …`.
/// No install command fixes either, so none is named.
/// `package-not-found` is answered by `package_not_found` instead.
fn sidecar_error(err: &str) -> ToolchainError {
    ToolchainError::Unusable {
        what: format!(
            "the type-aware sidecar failed to initialize the TypeScript program ({err}) — fix \
             the TypeScript toolchain in this project"
        ),
    }
}

/// The refusal when no typescript-go was resolvable at all.
/// `roots` are the global directories tried after the project.
/// Naming them answers where comply looked.
fn package_not_found(roots: &[PathBuf]) -> ToolchainError {
    let searched = if roots.is_empty() {
        ", and no global node_modules was found to fall back on".to_owned()
    } else {
        let named: Vec<String> = roots
            .iter()
            .map(|root| root.display().to_string())
            .collect();
        format!(", nor from any global install ({})", named.join(", "))
    };
    ToolchainError::Missing {
        what: format!(
            "type-aware analysis needs the typescript-go API, but none of typescript >= 7, \
             @typescript/native or @typescript/native-preview could be resolved from the \
             project{searched}"
        ),
        install_cmd: "npm install --save-dev typescript@^7".to_owned(),
    }
}

/// The refusal for `files`, which no TypeScript program holds.
/// Each is named by the path comply discovered.
fn unplaced_error(
    files: &[String],
    canon_to_orig: &rustc_hash::FxHashMap<String, PathBuf>,
) -> ToolchainError {
    let paths: Vec<String> = files
        .iter()
        .map(|file| {
            canon_to_orig
                .get(file)
                .map_or_else(|| file.clone(), |path| path.display().to_string())
        })
        .collect();
    ToolchainError::Unusable {
        what: format!(
            "type-aware analysis found no TypeScript program for {}: no tsconfig.json \
             includes it, directly or through `references`, and none sits above it — add it \
             to a tsconfig.json",
            quoted_files(paths)
        ),
    }
}

/// `paths`, sorted, the first [`QUOTED_UNPLACED`] named.
fn quoted_files(mut paths: Vec<String>) -> String {
    paths.sort();
    let more = paths.len().saturating_sub(QUOTED_UNPLACED);
    paths.truncate(QUOTED_UNPLACED);
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    format!("{}{tail}", paths.join(", "))
}

/// Map sidecar diagnostics into comply diagnostics at the rule's severity,
/// ignoring any unknown rule id.
fn map_diagnostics(
    diags: Vec<SidecarDiag>,
    enabled: &[&'static crate::rules::meta::RuleMeta],
    canon_to_orig: &rustc_hash::FxHashMap<String, PathBuf>,
) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .filter_map(|d| {
            let meta = enabled.iter().find(|m| m.id == d.rule)?;
            let severity = meta.severity;
            let path = canon_to_orig
                .get(&d.file)
                .cloned()
                .unwrap_or_else(|| PathBuf::from(&d.file));
            Some(Diagnostic {
                path: std::sync::Arc::from(path.as_path()),
                line: d.line.max(1),
                column: d.column.max(1),
                rule_id: std::borrow::Cow::Borrowed(meta.id),
                message: d.message,
                severity,
                span: None,
            })
        })
        .collect()
}

/// Removes the temp sidecar file on drop (best-effort).
struct TempFile<'a>(&'a Path);

impl Drop for TempFile<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use crate::rules::meta::RuleMeta;

    const REDUNDANT_NULLISH: &str = "no-redundant-nullish-coalescing-null";

    fn meta() -> &'static RuleMeta {
        Box::leak(Box::new(RuleMeta {
            id: REDUNDANT_NULLISH,
            description: "d",
            remediation: "r",
            severity: Severity::Error,
            doc_url: None,
            categories: &["typescript", "type-aware"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        }))
    }

    #[test]
    fn map_diagnostics_remaps_canonical_path_to_discovered() {
        let m = meta();
        let mut canon = rustc_hash::FxHashMap::default();
        canon.insert(
            "/private/tmp/proj/src/a.ts".to_string(),
            PathBuf::from("/tmp/proj/src/a.ts"),
        );
        let diags = vec![SidecarDiag {
            file: "/private/tmp/proj/src/a.ts".to_string(),
            line: 6,
            column: 19,
            rule: REDUNDANT_NULLISH.to_string(),
            message: "msg".to_string(),
        }];
        let out = map_diagnostics(diags, &[m], &canon);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path.to_string_lossy(), "/tmp/proj/src/a.ts");
        assert_eq!(out[0].line, 6);
        assert_eq!(out[0].column, 19);
        assert_eq!(out[0].rule_id, REDUNDANT_NULLISH);
    }

    #[test]
    fn map_diagnostics_drops_unknown_rule_ids() {
        let m = meta();
        let canon = rustc_hash::FxHashMap::default();
        let diags = vec![SidecarDiag {
            file: "/tmp/x.ts".to_string(),
            line: 1,
            column: 1,
            rule: "some-other-rule".to_string(),
            message: "msg".to_string(),
        }];
        let out = map_diagnostics(diags, &[m], &canon);
        assert!(out.is_empty(), "unknown rule ids must be ignored");
    }

    #[test]
    fn map_diagnostics_falls_back_to_reported_path_when_unmapped() {
        let m = meta();
        let canon = rustc_hash::FxHashMap::default();
        let diags = vec![SidecarDiag {
            file: "/abs/unmapped.ts".to_string(),
            line: 2,
            column: 3,
            rule: REDUNDANT_NULLISH.to_string(),
            message: "msg".to_string(),
        }];
        let out = map_diagnostics(diags, &[m], &canon);
        assert_eq!(out[0].path.to_string_lossy(), "/abs/unmapped.ts");
    }

    // An unplaced file is refused under its discovered path.
    #[test]
    fn unplaced_files_are_refused_by_their_discovered_path() {
        let mut canon = rustc_hash::FxHashMap::default();
        canon.insert(
            "/private/tmp/p/y.ts".to_string(),
            PathBuf::from("/tmp/p/y.ts"),
        );
        let err = unplaced_error(&["/private/tmp/p/y.ts".to_string()], &canon);
        assert!(matches!(err, ToolchainError::Unusable { .. }), "{err:?}");
        assert!(err.to_string().contains("for /tmp/p/y.ts:"), "{err}");
    }

    // The refusal names at most `QUOTED_UNPLACED` files.
    #[test]
    fn an_unplaced_refusal_counts_the_files_it_does_not_name() {
        let files: Vec<String> = (0..=QUOTED_UNPLACED)
            .map(|n| format!("/p/{n:02}.ts"))
            .collect();
        let msg = unplaced_error(&files, &rustc_hash::FxHashMap::default()).to_string();
        assert!(msg.contains("/p/19.ts and 1 more:"), "{msg}");
        assert!(!msg.contains("/p/20.ts"), "{msg}");
    }

    // ── fail-loud sidecar errors (#5941, #8109) ─────────────────────────────

    // Nothing resolvable anywhere is a hard error.
    // It carries the install command, so `main` instructs.
    // A crash banner would be the wrong answer.
    #[test]
    fn package_not_found_is_installable() {
        let err = package_not_found(&[]);
        assert!(
            matches!(&err, ToolchainError::Missing { install_cmd, .. }
                if install_cmd == "npm install --save-dev typescript@^7"),
            "{err:?}"
        );
        assert!(
            err.to_string()
                .starts_with("type-aware analysis needs the typescript-go API"),
            "{err}"
        );
        assert!(err.to_string().contains("@typescript/native or"), "{err}");
    }

    // Only the refusal can say where comply looked.
    // An unseen install is a question about the search.
    // A refusal that cannot answer it earns a symlink instead.
    #[test]
    fn package_not_found_names_the_global_roots_it_tried() {
        let tried = [
            PathBuf::from("/opt/npm/lib/node_modules"),
            PathBuf::from("/home/u/.volta/tools/image/packages/typescript/lib/node_modules"),
        ];
        let named = package_not_found(&tried).to_string();
        for root in &tried {
            assert!(named.contains(&root.display().to_string()), "{named}");
        }

        let nothing = package_not_found(&[]).to_string();
        assert!(
            nothing.contains("no global node_modules was found"),
            "{nothing}"
        );
    }

    // Volta keys one image directory per package name.
    // So one candidate per name, in the sidecar's order.
    #[test]
    fn volta_candidates_cover_every_typescript_go_package() {
        let roots = volta_module_roots(Path::new("/home/u/.volta"));
        assert_eq!(roots.len(), TYPESCRIPT_GO_PACKAGES.len());
        for (root, pkg) in roots.iter().zip(TYPESCRIPT_GO_PACKAGES) {
            assert_eq!(
                root,
                &PathBuf::from(format!(
                    "/home/u/.volta/tools/image/packages/{pkg}/lib/node_modules"
                )),
            );
        }
    }

    // A global bin is a symlink into its package.
    // Node then resolves from the `node_modules` above it.
    #[test]
    fn tsgo_on_path_yields_the_node_modules_it_was_installed_into() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir
            .path()
            .join("lib/node_modules/@typescript/native-preview/bin");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(installed.join("tsgo"), "#!/bin/sh\n").unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink(installed.join("tsgo"), bin.join("tsgo")).unwrap();

        let found = tsgo_module_root(bin.as_os_str()).expect("tsgo on PATH must resolve");
        assert_eq!(
            found.canonicalize().unwrap(),
            dir.path().join("lib/node_modules").canonicalize().unwrap(),
        );
    }

    // Volta's shim resolves to `volta-shim`, inside no package.
    // No ancestor means no root, rather than a wrong guess.
    #[test]
    fn a_tsgo_outside_any_node_modules_yields_no_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tsgo"), "#!/bin/sh\n").unwrap();
        assert!(tsgo_module_root(dir.path().as_os_str()).is_none());
    }

    // What the refusal names must be what was searched.
    // An absent candidate is dropped.
    // A directory two routes agree on is tried once.
    #[test]
    fn existing_unique_keeps_order_drops_the_absent_and_never_repeats() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();

        let kept = existing_unique(vec![
            first.clone(),
            dir.path().join("absent"),
            second.clone(),
            first.clone(),
        ]);
        assert_eq!(kept, vec![first, second]);
    }

    // An api-init / snapshot cause is a hard error too, but no single command
    // fixes it: it preserves the underlying cause and points at the toolchain.
    #[test]
    fn sidecar_error_api_init_preserves_cause() {
        let err = sidecar_error("api-init-failed: boom");
        assert!(matches!(err, ToolchainError::Unusable { .. }), "{err:?}");
        let msg = err.to_string();
        assert!(msg.contains("api-init-failed: boom"), "{msg}");
        assert!(msg.contains("toolchain"), "{msg}");
    }

    /// The custom type-aware rules are registered with `Backend::TypeAware` so
    /// they surface in `comply list`/`catalog` and feed the sidecar phase.
    #[test]
    fn custom_type_aware_rules_are_registered() {
        let metas = crate::rules::collect_type_aware_bindings();
        for id in [
            REDUNDANT_NULLISH,
            "no-duplicate-type-definition",
            "ts-no-in-operator",
            "ts-no-typeof-operator",
        ] {
            assert!(
                metas.iter().any(|m| m.id == id),
                "expected {id} among type-aware bindings"
            );
        }
    }
}
