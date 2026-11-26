//! oxlint subprocess — invokes oxlint on TS/JS files and converts JSON output
//! into unified Diagnostic structs.
//!
//! How it works:
//! 1. `is_available()` checks the binary is on PATH. Result is cached in a
//!    `OnceLock` so we don't fork oxlint on every invocation.
//! 2. `lint_files()` collects every `Backend::Oxlint` binding from the rule
//!    registry, passes them to `oxlint_config::generate` to produce the
//!    runtime config, then invokes `oxlint --format json -c <config>` with
//!    file paths terminated by `--` (so paths starting with `-` don't look
//!    like flags).
//! 3. Parses the JSON envelope from raw bytes.
//!    Each diagnostic's rule id and severity go through the registry.
//!    Users see `[no-explicit-any]`, not `typescript(no-explicit-any)`.

mod options;
mod remap;
mod schema;

pub use options::for_rule as options_for;

use anyhow::{Context, Result, bail};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;
use wait_timeout::ChildExt;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::project::ProjectCtx;
use crate::rules::meta::RuleMeta;
use crate::toolchain::{Unanalyzed, report_unanalyzed};
use schema::{OxlintDiag, OxlintOutput};

/// Max files per oxlint invocation. Conservative chunk size to avoid ARG_MAX.
const FILES_PER_BATCH: usize = 500;
const OXLINT_BATCH_TIMEOUT: Duration = Duration::from_secs(45);
/// How many directories above tsgolint may hold its `package.json`.
const PACKAGE_DEPTH: usize = 6;
/// A `version` longer than this is no version.
const VERSION_CHARS: usize = 64;
/// A `package.json` read stops past this size.
const PACKAGE_JSON_BYTES: u64 = 64 * 1024;
/// How much of a tsgolint panic line a message quotes.
const PANIC_CHARS: usize = 300;

/// Check if oxlint binary is on PATH. Result is cached for the process lifetime.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("oxlint")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Invoke oxlint on the given TS/JS files and return unified diagnostics.
///
/// `type_aware` gates the tsgolint rule set (and the `--type-aware` oxlint flag
/// that has oxlint-tsgolint build the TypeScript program). The caller sets it
/// only when a rule needing that program is still enabled; otherwise the pass
/// stays syntactic and fast.
#[must_use = "diagnostics from oxlint must be reported"]
pub fn lint_files(
    files: &[&SourceFile],
    config: &crate::config::Config,
    project: &ProjectCtx,
    type_aware: bool,
    type_programs: Option<&[TypeProgram]>,
    unanalyzed: Unanalyzed,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let oxlint_bindings = crate::rules::collect_oxlint_bindings();
    let tsgolint_bindings = if type_aware {
        crate::rules::collect_tsgolint_bindings()
    } else {
        Vec::new()
    };
    if oxlint_bindings.is_empty() && tsgolint_bindings.is_empty() {
        return Ok(vec![]);
    }
    let (module_aware_oxlint, standard_oxlint): (Vec<_>, Vec<_>) = oxlint_bindings
        .into_iter()
        .partition(|(key, _, _)| is_require_import_rule(key));
    let (module_aware_tsgolint, standard_tsgolint): (Vec<_>, Vec<_>) = tsgolint_bindings
        .into_iter()
        .partition(|(key, _, _)| is_require_import_rule(key));

    let mut all = Vec::new();
    if !standard_oxlint.is_empty() {
        all.extend(lint_files_with_bindings(
            files,
            config,
            &standard_oxlint,
            false,
            unanalyzed,
        )?);
    }

    let type_aware_ts = type_aware_files(files);
    if !type_aware_ts.is_empty() && !standard_tsgolint.is_empty() {
        if let Some(programs) = type_programs {
            // Partial-scan: pass all project TS files so tsgolint builds a
            // complete type program (prevents no-unsafe-* FPs on imports from
            // unchanged files), then filter results back to the changed files.
            all.extend(lint_programs(
                programs,
                &type_aware_ts,
                type_aware_files,
                &standard_tsgolint,
                config,
                unanalyzed,
            )?);
        } else {
            all.extend(lint_files_with_bindings(
                &type_aware_ts,
                config,
                &standard_tsgolint,
                true,
                unanalyzed,
            )?);
        }
    }

    let esm = es_module_files(files, project);
    if !esm.is_empty() && !module_aware_oxlint.is_empty() {
        all.extend(lint_files_with_bindings(
            &esm,
            config,
            &module_aware_oxlint,
            false,
            unanalyzed,
        )?);
    }

    let type_aware_esm = type_aware_files(&esm);
    if !type_aware_esm.is_empty() && !module_aware_tsgolint.is_empty() {
        if let Some(programs) = type_programs {
            all.extend(lint_programs(
                programs,
                &type_aware_esm,
                |program| type_aware_files(&es_module_files(program, project)),
                &module_aware_tsgolint,
                config,
                unanalyzed,
            )?);
        } else {
            all.extend(lint_files_with_bindings(
                &type_aware_esm,
                config,
                &module_aware_tsgolint,
                true,
                unanalyzed,
            )?);
        }
    }

    let mut source_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();

    // Generated and minified files are exempt from every rule, native and
    // delegated alike (the native engine drops them at `dispatch_with_lang` via
    // the `is_generated` / `is_minified` gate). oxlint runs as an external
    // subprocess that never sees that gate, so apply the same exemption to its
    // diagnostics here. A path signal (e.g. `routeTree.gen.ts`, a `generated/`
    // segment) is decisive on its own; otherwise scan the source for a codegen
    // banner (e.g. ANTLR's `// Generated from … by …`) or minified/compiled
    // content (e.g. suffix-less Emscripten output whose lines run thousands of
    // characters), so the delegated rules drop them exactly as the engine does.
    all.retain(|d| {
        if crate::rules::file_ctx::is_generated_path(d.path.as_ref()) {
            return false;
        }
        let source = source_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok())
            .as_deref();
        let Some(source) = source else {
            return true;
        };
        !crate::rules::file_ctx::is_generated_content(source)
            && !crate::rules::file_ctx::scan_minified(d.path.as_ref(), source)
    });

    let filters = crate::rules::collect_delegated_post_filters();
    if !filters.is_empty() {
        all.retain(|d| {
            let Some(filter_vec) = filters.get(d.rule_id.as_ref()) else {
                return true;
            };
            let source = source_cache
                .entry(d.path.to_path_buf())
                .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok())
                .as_deref();
            filter_vec
                .iter()
                .all(|f| f.keep_with_project(d, source, project))
        });
    }

    all.retain(|d| keep_delegated_diagnostic(d.rule_id.as_ref(), d.path.as_ref()));

    all.retain(|d| keep_jest_no_export(d, project));

    Ok(all)
}

/// `jest-no-export` flags every `export` from a `.spec.`/`.test.` file because a
/// test runner would re-execute the file's tests when another module imports it.
/// But a spec file that doubles as a shared-fixture provider — co-locating a
/// fixture (e.g. node-redis's `MATH_FUNCTION` Lua-script definition) and a helper
/// (`loadMathFunction`) with the command it tests, then exporting them to sibling
/// spec files — exports those bindings on purpose.
///
/// The exemption is file-level: when *any* of the file's exports is imported by
/// *another* test file, every `jest-no-export` diagnostic on that file is
/// dropped. A cross-spec import is the signal that the file is a deliberate
/// shared-fixture provider, not a leak. Imports from non-test files are not
/// considered — a production module importing a test file's export is a separate
/// problem and must not silence this rule.
///
/// Limitation: a fixture consumed only through a namespace import
/// (`import * as ns from './foo.spec'`) is not seen here — `get_usages` does not
/// record namespace imports per name — so such a file is still flagged.
fn keep_jest_no_export(diag: &Diagnostic, project: &ProjectCtx) -> bool {
    if diag.rule_id.as_ref() != "jest-no-export" {
        return true;
    }
    let index = project.import_index();
    // Single-file invocations (LSP, `comply path/to/file.spec.ts`) index only the
    // checked file, so no cross-spec consumer can ever be seen. Fall back to the
    // default behaviour rather than silence a real leak we simply can't disprove.
    if index.total_files() < 2 {
        return true;
    }
    let canon = index.canonical(diag.path.as_ref());
    let imported_by_another_test_file = index.get_exports(&canon).iter().any(|export| {
        index
            .get_usages(&canon, &export.name)
            .iter()
            .any(|usage| usage.importer != canon && is_jest_test_file(&usage.importer))
    });
    !imported_by_another_test_file
}

/// A tsgolint program, and the changed files it owns.
pub struct TypeProgram {
    /// One package root's changed files, then its other files.
    pub files: Vec<SourceFile>,
    /// The canonical paths of the changed files this program owns.
    pub changed: FxHashSet<PathBuf>,
}

/// Run tsgolint `bindings` on each program's `select`ed files.
/// Each keeps the findings of the `changed` files it owns.
/// A changed file no program owns is not analyzed.
fn lint_programs(
    programs: &[TypeProgram],
    changed: &[&SourceFile],
    select: impl for<'f> Fn(&[&'f SourceFile]) -> Vec<&'f SourceFile>,
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
    config: &crate::config::Config,
    unanalyzed: Unanalyzed,
) -> Result<Vec<Diagnostic>> {
    let outside: Vec<&SourceFile> = changed
        .iter()
        .copied()
        .filter(|file| {
            let is_held = |program: &TypeProgram| is_changed(&program.changed, &file.path);
            !programs.iter().any(is_held)
        })
        .collect();
    if !outside.is_empty() {
        let why = "no type program holds these changed file(s)";
        report_unanalyzed(why, &outside, unanalyzed)?;
    }
    let rule_entries: Vec<crate::oxlint_config::RuleEntry<'_>> = bindings
        .iter()
        .map(|(key, _, sev)| (*key, *sev, options::for_rule(key, config)))
        .collect();
    let oxlint_config = crate::oxlint_config::generate(&rule_entries)?;
    let remap = remap::build_table(bindings);
    let mut all = Vec::new();
    for program in programs {
        let owned: Vec<&SourceFile> = changed
            .iter()
            .copied()
            .filter(|file| is_changed(&program.changed, &file.path))
            .collect();
        if owned.is_empty() {
            continue;
        }
        let files: Vec<&SourceFile> = program.files.iter().collect();
        let diagnostics = lint_batch(
            &select(&files),
            &owned,
            oxlint_config.path(),
            true,
            &remap,
            unanalyzed,
        )?;
        let kept = changed_path_set(&owned);
        all.extend(
            diagnostics
                .into_iter()
                .filter(|d| is_changed(&kept, d.path.as_ref())),
        );
    }
    Ok(all)
}

fn changed_path_set(files: &[&SourceFile]) -> FxHashSet<PathBuf> {
    files
        .iter()
        .map(|f| std::fs::canonicalize(&f.path).unwrap_or_else(|_| f.path.clone()))
        .collect()
}

/// True when a diagnostic names a changed file.
/// oxlint echoes each path as passed, often relative.
/// `changed` is canonical, so canonicalize the path too.
fn is_changed(changed: &FxHashSet<PathBuf>, path: &Path) -> bool {
    changed.contains(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

fn is_require_import_rule(key: &str) -> bool {
    matches!(key, "typescript/no-require-imports" | "no-require-imports")
}

/// The delegated eslint-plugin-jest rule ids (see `rules::delegated::jest`).
/// oxlint runs them on every TS/JS file, but they only make sense on files
/// the test runner executes.
fn is_jest_delegated_rule(rule_id: &str) -> bool {
    matches!(rule_id, "jest-no-export" | "jest-consistent-test-it")
}

/// True for a file the jest/vitest runner actually executes: its name carries a
/// `.test.`/`.spec.` infix (case-insensitive), e.g. `foo.test.ts`,
/// `foo.spec.tsx`. A helper module that merely re-exports a parametrised
/// `it`/`test` wrapper (`test-helpers/tx-test.ts`) has no such infix and is not
/// a test file.
fn is_jest_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.contains(".test.") || name.contains(".spec.")
        })
}

/// eslint-plugin-jest rules are only meaningful on actual jest/vitest test
/// files (executed by the runner), not on helper modules that re-export a
/// parametrised `it`/`test` wrapper. A delegated `jest-*` diagnostic on a
/// non-test file (e.g. a `test-helpers/tx-test.ts` wrapper) is a false
/// positive, so keep jest diagnostics only on test files.
fn keep_delegated_diagnostic(rule_id: &str, path: &Path) -> bool {
    !is_jest_delegated_rule(rule_id) || is_jest_test_file(path)
}

fn es_module_files<'a>(files: &[&'a SourceFile], project: &ProjectCtx) -> Vec<&'a SourceFile> {
    files
        .iter()
        .copied()
        .filter(|file| crate::rules::module_system::is_es_module_context(&file.path, project))
        .collect()
}

fn type_aware_files<'a>(files: &[&'a SourceFile]) -> Vec<&'a SourceFile> {
    files
        .iter()
        .copied()
        .filter(|file| {
            matches!(
                file.language,
                crate::files::Language::TypeScript | crate::files::Language::Tsx
            )
        })
        .collect()
}

fn lint_files_with_bindings(
    files: &[&SourceFile],
    config: &crate::config::Config,
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
    type_aware: bool,
    unanalyzed: Unanalyzed,
) -> Result<Vec<Diagnostic>> {
    let rule_entries: Vec<crate::oxlint_config::RuleEntry> = bindings
        .iter()
        .map(|(key, _, sev)| (*key, *sev, options::for_rule(key, config)))
        .collect();
    let oxlint_config = crate::oxlint_config::generate(&rule_entries)?;
    let remap = remap::build_table(bindings);

    let mut all = Vec::with_capacity(files.len());
    for batch in files.chunks(FILES_PER_BATCH) {
        all.extend(lint_batch(
            batch,
            batch,
            oxlint_config.path(),
            type_aware,
            &remap,
            unanalyzed,
        )?);
    }
    Ok(all)
}

/// Run oxlint over `batch` and return its diagnostics.
/// A report on a missed batch names `named`.
fn lint_batch(
    batch: &[&SourceFile],
    named: &[&SourceFile],
    config_path: &Path,
    is_type_aware: bool,
    remap: &FxHashMap<String, &'static RuleMeta>,
    unanalyzed: Unanalyzed,
) -> Result<Vec<Diagnostic>> {
    let Some(output) = invoke_oxlint(batch, Some(config_path), is_type_aware)? else {
        let limit = OXLINT_BATCH_TIMEOUT.as_secs();
        let why = format!(
            "oxlint ran past {limit}s on a batch of {} file(s)",
            batch.len()
        );
        report_unanalyzed(&why, named, unanalyzed)?;
        return Ok(Vec::new());
    };
    match parse_oxlint_output(&output.stdout, &output.stderr, remap)? {
        Parsed::Panicked(panic) => {
            let why = format!(
                "the type-aware checker (tsgolint {}) panicked on a batch of {} file(s): \
                 {panic}. Pin another oxlint-tsgolint, or report the panic upstream",
                tsgolint_version(),
                batch.len()
            );
            report_unanalyzed(&why, named, unanalyzed)?;
            Ok(Vec::new())
        }
        Parsed::Linted(diagnostics, Some(linted)) if linted < batch.len() => {
            let why = format!(
                "oxlint linted only {linted} of the {} file(s) of a batch, without naming them; \
                 an ignore file it reads is the usual cause",
                batch.len()
            );
            report_unanalyzed(&why, named, unanalyzed)?;
            Ok(diagnostics)
        }
        Parsed::Linted(diagnostics, _) => Ok(diagnostics),
    }
}

/// The version of the tsgolint oxlint runs, from its `package.json`.
/// oxlint looks in `OXLINT_TSGOLINT_PATH`, then `node_modules/.bin`, then `PATH`.
fn tsgolint_version() -> String {
    let named = std::env::var_os("OXLINT_TSGOLINT_PATH").map(PathBuf::from);
    let in_project: Vec<PathBuf> = std::env::current_dir()
        .map(|cwd| {
            cwd.ancestors()
                .map(|dir| dir.join("node_modules/.bin/tsgolint"))
                .collect()
        })
        .unwrap_or_default();
    let on_path: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("tsgolint"))
                .collect()
        })
        .unwrap_or_default();
    let version = named
        .into_iter()
        .chain(in_project)
        .chain(on_path)
        .find(|candidate| candidate.is_file())
        .and_then(|bin| bin.canonicalize().ok())
        .and_then(|bin| {
            bin.ancestors()
                .skip(1)
                .take(PACKAGE_DEPTH)
                .find_map(tsgolint_package_version)
        });
    version.unwrap_or_else(|| "of unknown version".to_owned())
}

/// The `version` of `dir/package.json` when it is a tsgolint package.
fn tsgolint_package_version(dir: &Path) -> Option<String> {
    let mut text = String::new();
    std::fs::File::open(dir.join("package.json"))
        .ok()?
        .take(PACKAGE_JSON_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    let package: serde_json::Value = serde_json::from_str(&text).ok()?;
    if !package.get("name")?.as_str()?.contains("tsgolint") {
        return None;
    }
    let version = package.get("version")?.as_str()?;
    Some(version.chars().take(VERSION_CHARS).collect())
}

/// Spawn oxlint as a subprocess and validate exit status.
///
/// `--type-aware` is only passed for tsgolint batches: it forces oxlint to
/// build the TypeScript program (slow), so the syntactic-rule batches run
/// without it.
/// `None` when oxlint ran past `OXLINT_BATCH_TIMEOUT`.
fn invoke_oxlint(
    files: &[&SourceFile],
    config_path: Option<&Path>,
    type_aware: bool,
) -> Result<Option<std::process::Output>> {
    let mut cmd = Command::new("oxlint");
    cmd.args(["--format", "json"]);
    if type_aware {
        cmd.arg("--type-aware");
    }
    if let Some(cfg) = config_path {
        cmd.arg("-c").arg(cfg);
    }
    // `--` terminates option parsing so file paths starting with `-` are not
    // interpreted as flags by oxlint.
    cmd.arg("--");
    for f in files {
        cmd.arg(&f.path);
    }

    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to invoke oxlint — install it with: npm install -g oxlint")?;
    let Some(output) = drain_and_wait(child, OXLINT_BATCH_TIMEOUT, "oxlint")? else {
        return Ok(None);
    };

    // oxlint exits 1 when violations are found — that is normal, not an error.
    if !output.status.success() && output.status.code() != Some(1) {
        bail!(
            "oxlint crashed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(Some(output))
}

/// Wait for `child` up to `timeout`, draining its stdout/stderr on dedicated
/// threads. Returns `Ok(None)` if the child was killed for exceeding `timeout`.
///
/// The threads are load-bearing: `wait_timeout` only polls the process and
/// never reads the pipes, so a child that writes past the OS pipe buffer
/// (~64 KiB) blocks on `write()` and can never exit — a deadlock that surfaces
/// as a spurious timeout on any project with enough diagnostics to overflow it.
pub(crate) fn drain_and_wait(
    mut child: std::process::Child,
    timeout: Duration,
    tool: &str,
) -> Result<Option<std::process::Output>> {
    let mut stdout_pipe = child.stdout.take().expect("child stdout must be piped");
    let mut stderr_pipe = child.stderr.take().expect("child stderr must be piped");
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let Some(status) = child
        .wait_timeout(timeout)
        .with_context(|| format!("failed to wait for {tool}"))?
    else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Ok(None);
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    Ok(Some(std::process::Output {
        status,
        stdout,
        stderr,
    }))
}

/// Parse oxlint JSON output bytes into unified Diagnostic structs.
///
/// Two recoveries sit ahead of the error path, because one file oxlint chokes
/// on must not cost the whole batch its diagnostics:
///
/// 1. oxlint's `--format json` writer copies source text into `message`
///    without escaping it, so a file holding a raw control byte (an ANSI
///    golden fixture, say) yields JSON that RFC 8259 forbids. The output is
///    otherwise well-formed, so a rewrite that escapes those bytes is retried
///    before giving up. This recovery is silent: it is complete for every
///    diagnostic comply keeps.
/// 2. The type-aware backend (tsgolint, written in Go) can panic on a
///    TypeScript shape it does not handle — e.g. an ambient `module "x";`
///    declaration (`KindModuleDeclaration` in a `MemberList`). The panic
///    crashes the oxlint subprocess, leaving non-JSON output. The caller
///    reports the batch as not analyzed.
///
/// Anything else — truncated output, a changed envelope shape — still errors.
fn parse_oxlint_output(
    stdout: &[u8],
    stderr: &[u8],
    remap: &FxHashMap<String, &'static RuleMeta>,
) -> Result<Parsed> {
    let json = envelope_start(stdout);
    let envelope: OxlintOutput = match serde_json::from_slice(json) {
        Ok(envelope) => envelope,
        Err(parse_err) => match serde_json::from_str(&escape_control_bytes(json)) {
            Ok(envelope) => envelope,
            Err(_) => {
                if let Some(panic) = tsgolint_panic_summary(stdout, stderr) {
                    return Ok(Parsed::Panicked(panic));
                }
                // The first error is the one that describes the real output;
                // the retry only ever fails for the same reason or worse.
                return Err(parse_err).with_context(|| {
                    format!(
                        "failed to parse oxlint JSON output. oxlint stderr: {}",
                        String::from_utf8_lossy(stderr)
                    )
                });
            }
        },
    };
    let diagnostics = envelope
        .diagnostics
        .into_iter()
        .filter_map(|d| into_diagnostic(d, remap))
        .collect();
    Ok(Parsed::Linted(diagnostics, envelope.number_of_files))
}

/// What oxlint made of one batch.
enum Parsed {
    /// Its diagnostics, and how many files it says it linted.
    Linted(Vec<Diagnostic>, Option<usize>),
    /// tsgolint panicked: the first line of its panic.
    Panicked(String),
}

/// `stdout` from its first line that opens the JSON envelope.
/// oxlint 1.90 prints "No files found to lint" ahead of it.
fn envelope_start(stdout: &[u8]) -> &[u8] {
    if stdout.trim_ascii_start().starts_with(b"{") {
        return stdout;
    }
    stdout
        .windows(2)
        .position(|pair| pair == b"\n{")
        .map_or(stdout, |newline| &stdout[newline..])
}

/// Rewrite the `U+0000`–`U+001F` bytes that sit *inside* JSON strings so the
/// envelope parses, leaving everything else byte-identical.
///
/// The replacement is a doubled backslash — the recovered message reads
/// `\u001b` as six printable characters. A plain JSON escape would decode back
/// to the control byte and put it in `OxlintDiag::message`, which comply prints
/// to the terminal and the delegated post-filters match on; source text is
/// attacker-controlled, so it must not reach either as a live escape sequence.
///
/// Bytes outside strings stay untouched: oxlint's envelope really does hold a
/// newline between the diagnostics array and the trailing metadata, and that
/// whitespace is legal JSON. Decoding is lossy, which also covers oxlint
/// stdout that is not valid UTF-8; multi-byte sequences are above `0x20` and
/// pass through whole.
fn escape_control_bytes(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout);
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    // True for exactly the one character following an unescaped backslash, so
    // a `\"` inside a string is not mistaken for the string's closing quote.
    let mut after_backslash = false;
    for ch in text.chars() {
        if after_backslash {
            after_backslash = false;
            out.push(ch);
            continue;
        }
        match ch {
            '\\' if in_string => {
                after_backslash = true;
                out.push(ch);
            }
            '"' => {
                in_string = !in_string;
                out.push(ch);
            }
            control if in_string && (control as u32) < 0x20 => {
                let _ = write!(out, "\\\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out
}

/// Detect a tsgolint/typescript-go (Go) panic in the subprocess output and
/// return its first line for the warning. Returns `None` for any other parse
/// failure so genuine oxlint output-format breakage still surfaces as an error.
///
/// A Go panic prints `panic: <msg>` followed by a `goroutine … [running]:` stack
/// referencing `typescript-go` or `tsgolint`; both markers are required so an
/// ordinary diagnostic message containing the word "panic" is not misread.
fn tsgolint_panic_summary(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let combined = format!("{stdout}\n{stderr}");
    let panic_line = combined
        .lines()
        .find(|l| l.trim_start().starts_with("panic:"))?;
    let is_go_panic = combined.contains("goroutine ")
        && (combined.contains("typescript-go") || combined.contains("tsgolint"));
    if !is_go_panic {
        return None;
    }
    Some(panic_line.trim().chars().take(PANIC_CHARS).collect())
}

/// Convert one oxlint diagnostic into our unified format, remapping the
/// rule_id + severity through the registry when a match exists.
///
/// Returns `None` for a diagnostic with no `code` — oxlint emits those for
/// parser/syntax errors, not lint-rule violations, so they are dropped. comply
/// routes `.js` files to oxlint's JavaScript parser; a `.js` file carrying
/// TypeScript type annotations (type-stripped JS, common in Vite projects)
/// fails that parse and yields a code-less error that is not a lint finding.
fn into_diagnostic(
    d: OxlintDiag,
    remap: &FxHashMap<String, &'static RuleMeta>,
) -> Option<Diagnostic> {
    let oxlint_code = d.code?;

    let (line, column) = d
        .labels
        .first()
        .map(|l| (l.span.line.max(1), l.span.column.max(1)))
        .unwrap_or((1, 1));

    let meta = remap
        .get(remap::current_code(&oxlint_code).as_ref())
        .copied();
    let (rule_id, severity) = match meta {
        Some(meta) => (std::borrow::Cow::Borrowed(meta.id), meta.severity),
        None => (std::borrow::Cow::Owned(oxlint_code), Severity::Error),
    };

    Some(Diagnostic {
        path: std::sync::Arc::from(std::path::PathBuf::from(d.filename).as_path()),
        line,
        column,
        rule_id,
        message: d.message,
        severity,
        span: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The diagnostics of a batch oxlint linted.
    /// A tsgolint panic is an error here.
    fn parse_json_bytes(
        stdout: &[u8],
        stderr: &[u8],
        remap: &FxHashMap<String, &'static RuleMeta>,
    ) -> Result<Vec<Diagnostic>> {
        match parse_oxlint_output(stdout, stderr, remap)? {
            Parsed::Linted(diagnostics, _) => Ok(diagnostics),
            Parsed::Panicked(panic) => bail!("tsgolint panicked: {panic}"),
        }
    }

    // Manual integration test for issue #755:
    // 1. Create project with A.ts (unchanged, exports typed hook) and B.tsx (changed, imports A)
    // 2. Run `comply --working-tree --type-aware` on B.tsx only
    // 3. Expect: no no-unsafe-* errors in B.tsx
    // Reproducer from issue: mutations.ts with comply-ignore-file: unused-file

    #[test]
    fn changed_path_set_retains_changed_files_and_drops_program_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let changed_path = dir.path().join("B.tsx");
        let unchanged_path = dir.path().join("A.ts");
        std::fs::write(&changed_path, "").unwrap();
        std::fs::write(&unchanged_path, "").unwrap();

        let changed_sf = SourceFile {
            path: changed_path.clone(),
            language: crate::files::Language::Tsx,
        };
        let set = changed_path_set(&[&changed_sf]);

        // B.tsx (changed) resolves and is present
        assert!(
            set.contains(&std::fs::canonicalize(&changed_path).unwrap()),
            "changed file must be in set"
        );
        // A.ts (unchanged program file) is absent
        assert!(
            !set.contains(&std::fs::canonicalize(&unchanged_path).unwrap()),
            "unchanged file must not be in set"
        );

        // Diagnostics from B.tsx survive; diagnostics from A.ts are dropped.
        // oxlint keeps each path as passed.
        let make_diag = |path: &std::path::Path| crate::diagnostic::Diagnostic {
            path: std::sync::Arc::from(path),
            line: 1,
            column: 1,
            rule_id: std::borrow::Cow::Borrowed("no-unsafe-assignment"),
            message: "unsafe".into(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        };
        let diags = vec![make_diag(&changed_path), make_diag(&unchanged_path)];
        let filtered: Vec<_> = diags
            .into_iter()
            .filter(|d| is_changed(&set, d.path.as_ref()))
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path.as_ref(), changed_path.as_path());
    }

    #[test]
    fn a_changed_file_no_program_owns_is_refused() {
        let file = SourceFile {
            path: "pkg-b/b.ts".into(),
            language: crate::files::Language::TypeScript,
        };
        let config = crate::config::Config::default();
        let Err(error) = lint_programs(
            &[],
            &[&file],
            type_aware_files,
            &[],
            &config,
            Unanalyzed::Refuse,
        ) else {
            panic!("an unowned changed file passed");
        };
        let message = error.to_string();
        assert!(message.contains("no type program"), "{message}");
        assert!(message.contains("pkg-b/b.ts"), "{message}");
    }

    #[test]
    fn fallback_position_is_one_one_not_zero_zero() {
        let remap = FxHashMap::default();
        let json = br#"{ "diagnostics": [{"message": "X", "code": "eslint(no-debugger)", "severity": "error", "filename": "/tmp/x.ts", "labels": []}] }"#;
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn parser_error_without_code_is_dropped() {
        // oxlint emits a code-less diagnostic when a file fails to parse — e.g.
        // a `.js` file carrying TypeScript type annotations routed to oxlint's
        // JavaScript parser (issue #4930). It is not a lint finding, so it must
        // not surface as a diagnostic.
        let remap = FxHashMap::default();
        let json = br#"{ "diagnostics": [{"message": "Expected a semicolon or an implicit semicolon after a statement, but found none", "severity": "error", "filename": "/tmp/DOM.js", "labels": [{"span": {"line": 1, "column": 15}}]}] }"#;
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert!(
            result.is_empty(),
            "code-less parser error must be dropped, got {result:?}"
        );
    }

    #[test]
    fn lint_finding_with_code_is_kept() {
        // A genuine oxlint lint finding carries a rule `code` and must still be
        // reported even when no remap entry matches (surfaced under its oxlint
        // code), so dropping parser errors does not silence real diagnostics.
        let remap = FxHashMap::default();
        let json = br#"{ "diagnostics": [{"message": "`debugger` statement is not allowed", "code": "eslint(no-debugger)", "severity": "error", "filename": "/tmp/app.js", "labels": [{"span": {"line": 2, "column": 1}}]}] }"#;
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule_id.as_ref(), "eslint(no-debugger)");
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn oxlint_190_and_159_codes_name_the_comply_rule() {
        // oxlint 1.90 writes `typescript(…)`, oxlint 1.59 wrote `typescript-eslint(…)`.
        let mut bindings = crate::rules::collect_oxlint_bindings();
        bindings.extend(crate::rules::collect_tsgolint_bindings());
        let remap = remap::build_table(&bindings);
        for (code, id) in [
            ("typescript(no-floating-promises)", "no-floating-promises"),
            (
                "typescript-eslint(no-floating-promises)",
                "no-floating-promises",
            ),
            ("react-hooks(exhaustive-deps)", "react-exhaustive-deps"),
            (
                "eslint-plugin-react-hooks(exhaustive-deps)",
                "react-exhaustive-deps",
            ),
            ("eslint(eqeqeq)", "eqeqeq"),
        ] {
            let json = format!(
                r#"{{ "diagnostics": [{{"message": "X", "code": "{code}", "severity": "error", "filename": "/tmp/x.ts", "labels": []}}] }}"#
            );
            let diagnostics = parse_json_bytes(json.as_bytes(), b"", &remap).expect("must parse");
            assert_eq!(diagnostics[0].rule_id.as_ref(), id, "{code}");
        }
    }

    #[test]
    fn empty_diagnostics_array_yields_empty_vec() {
        let remap = FxHashMap::default();
        let json = br#"{ "diagnostics": [] }"#;
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert!(result.is_empty());
    }

    #[test]
    fn a_tsgolint_panic_is_told_apart_from_corrupt_output_issue_5458() {
        // An ambient `module "abc";` declaration makes tsgolint's no-mixed-enums
        // rule panic on a KindModuleDeclaration in a MemberList, leaving non-JSON
        // output. The caller reports the batch as not analyzed.
        let remap = FxHashMap::default();
        let stderr = b"panic: Unhandled case in Node.MemberList: KindModuleDeclaration [recovered, repanicked]\n\
            \n\
            goroutine 2020 [running]:\n\
            github.com/microsoft/typescript-go/internal/ast.(*Node).MemberList(...)\n\
            \ttypescript-go/internal/ast/ast.go:559\n\
            github.com/typescript-eslint/tsgolint/internal/rules/no_mixed_enums.init.func1.2(...)\n";
        let parsed = parse_oxlint_output(b"", stderr, &remap).expect("a panic parses");
        let Parsed::Panicked(panic) = parsed else {
            panic!("a tsgolint panic must read as one");
        };
        assert_eq!(
            panic,
            "panic: Unhandled case in Node.MemberList: KindModuleDeclaration [recovered, repanicked]"
        );
    }

    #[test]
    fn the_envelope_after_a_no_files_notice_still_parses() {
        let remap = FxHashMap::default();
        let stdout = b"No files found to lint. Please check your paths and ignore patterns.\n\
            {\"diagnostics\": [], \"number_of_files\": 0}";
        let parsed = parse_oxlint_output(stdout, b"", &remap).expect("must parse");
        assert!(
            matches!(parsed, Parsed::Linted(ref diagnostics, Some(0)) if diagnostics.is_empty())
        );
    }

    #[test]
    fn non_panic_parse_failure_still_errors() {
        // Truncated/corrupt oxlint output with no Go panic signature must still
        // surface as an error. `{ not json` holds no control byte, so the escape
        // retry rewrites nothing and cannot turn this into a false recovery.
        let remap = FxHashMap::default();
        let result = parse_json_bytes(b"{ not json", b"some unrelated stderr", &remap);
        assert!(
            result.is_err(),
            "a non-panic parse failure must still error"
        );
    }

    #[test]
    fn raw_control_byte_in_a_message_is_recovered_issue_8402() {
        // oxlint's json writer copies source text into `message` unescaped, so a
        // golden fixture whose bytes start with ESC (0x1B) makes the envelope
        // illegal JSON. Its batch-mates — up to 500 files — must survive it.
        let remap = FxHashMap::default();
        let json = b"{ \"diagnostics\": [{\"message\": \"Invalid Character `\x1b`\", \
            \"code\": \"eslint(no-control-regex)\", \"severity\": \"error\", \
            \"filename\": \"/tmp/ansi.ts\", \"labels\": [{\"span\": {\"line\": 4, \"column\": 2}}]}] }";
        let result = parse_json_bytes(json, b"", &remap).expect("control byte must be recovered");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 4);
        assert_eq!(result[0].column, 2);
        // comply prints `message` to the terminal and the delegated post-filters
        // match on it, so a recovered control byte must arrive as printable text.
        assert!(
            !result[0].message.chars().any(char::is_control),
            "recovered message must stay printable, got {:?}",
            result[0].message
        );
    }

    #[test]
    fn structural_whitespace_survives_the_control_byte_rewrite() {
        // Real oxlint output puts a newline between diagnostics and before the
        // trailing metadata. That whitespace is outside any string and legal
        // JSON — escaping it would break the very retry it has to allow.
        let remap = FxHashMap::default();
        let json = b"{ \"diagnostics\": [{\"message\": \"esc \x1b here\", \
            \"code\": \"eslint(no-debugger)\", \"severity\": \"error\", \
            \"filename\": \"/tmp/x.ts\", \"labels\": [{\"span\": {\"line\": 1, \"column\": 1}}]}],\n\
            \"number_of_files\": 1 }";
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn escaped_quote_does_not_end_the_string_during_the_rewrite() {
        // A `\"` inside a message is not the closing quote. Reading it as one
        // would leave the rewrite outside string state at the control byte and
        // the retry would fail exactly like the first parse.
        let remap = FxHashMap::default();
        let json = b"{ \"diagnostics\": [{\"message\": \"says \\\" then \x1b\", \
            \"code\": \"eslint(no-debugger)\", \"severity\": \"error\", \
            \"filename\": \"/tmp/x.ts\", \"labels\": [{\"span\": {\"line\": 2, \"column\": 3}}]}] }";
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.contains('"'),
            "the quote is kept verbatim"
        );
        assert!(!result[0].message.chars().any(char::is_control));
    }

    #[test]
    fn diagnostic_message_containing_panic_word_still_parses() {
        // A real diagnostic whose message merely contains "panic" is valid JSON
        // and must parse normally — the degradation path is only taken when JSON
        // parsing itself fails on a Go panic.
        let remap = FxHashMap::default();
        let json = br#"{ "diagnostics": [{"message": "do not call process.exit on panic", "code": "eslint(no-process-exit)", "severity": "error", "filename": "/tmp/x.ts", "labels": [{"span": {"line": 3, "column": 1}}]}] }"#;
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn jest_diagnostics_dropped_on_non_test_files() {
        // The `tx-test.ts` wrapper helper and an ordinary source file are not
        // executed by the runner, so jest diagnostics on them are FPs.
        assert!(!keep_delegated_diagnostic(
            "jest-no-export",
            Path::new("src/api/test-helpers/tx-test.ts")
        ));
        assert!(!keep_delegated_diagnostic(
            "jest-consistent-test-it",
            Path::new("src/foo.ts")
        ));
    }

    #[test]
    fn jest_diagnostics_kept_on_real_test_files() {
        assert!(keep_delegated_diagnostic(
            "jest-no-export",
            Path::new("src/foo.test.ts")
        ));
        assert!(keep_delegated_diagnostic(
            "jest-consistent-test-it",
            Path::new("src/foo.spec.tsx")
        ));
    }

    #[test]
    fn non_jest_delegated_diagnostics_untouched_on_non_test_files() {
        assert!(keep_delegated_diagnostic(
            "no-floating-promises",
            Path::new("src/foo.ts")
        ));
    }

    #[test]
    fn drain_and_wait_does_not_deadlock_on_output_exceeding_pipe_buffer() {
        // A child that writes ~200 KiB — far past the ~64 KiB OS pipe buffer —
        // then exits 0. Without draining the pipes on threads, oxlint blocks on
        // write() and `wait_timeout` never sees it exit (issue: svix-webhooks
        // timed out at 45s despite finishing in <1s). It must complete, not
        // time out, and its full output must be captured.
        let child = Command::new("sh")
            .args([
                "-c",
                "i=0; while [ $i -lt 4000 ]; do printf '%050d\\n' $i; i=$((i + 1)); done",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sh");

        let output = drain_and_wait(child, Duration::from_secs(30), "sh")
            .expect("wait must not error")
            .expect("child must complete, not time out");

        assert!(output.status.success());
        assert!(
            output.stdout.len() > 200_000,
            "expected full output, got {} bytes",
            output.stdout.len()
        );
    }

    #[test]
    fn type_aware_files_exclude_plain_javascript() {
        let js = SourceFile {
            path: "fastify.js".into(),
            language: crate::files::Language::JavaScript,
        };
        let mjs = SourceFile {
            path: "plugin.mjs".into(),
            language: crate::files::Language::JavaScript,
        };
        let ts = SourceFile {
            path: "server.ts".into(),
            language: crate::files::Language::TypeScript,
        };
        let tsx = SourceFile {
            path: "view.tsx".into(),
            language: crate::files::Language::Tsx,
        };
        let files = [&js, &mjs, &ts, &tsx];

        let type_aware = type_aware_files(&files);

        assert_eq!(type_aware.len(), 2);
        assert!(
            type_aware
                .iter()
                .any(|file| file.path.ends_with("server.ts"))
        );
        assert!(
            type_aware
                .iter()
                .any(|file| file.path.ends_with("view.tsx"))
        );
    }

    #[test]
    fn es_module_files_respect_extensions_and_package_type() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
        let module_js = SourceFile {
            path: dir.path().join("src").join("module.js"),
            language: crate::files::Language::JavaScript,
        };
        let mjs = SourceFile {
            path: dir.path().join("standalone.mjs"),
            language: crate::files::Language::JavaScript,
        };
        let cjs = SourceFile {
            path: dir.path().join("legacy.cjs"),
            language: crate::files::Language::JavaScript,
        };
        let outside_js = SourceFile {
            path: dir.path().with_file_name("outside.js"),
            language: crate::files::Language::JavaScript,
        };
        let project = crate::project::ProjectCtx::empty();
        let files = [&module_js, &mjs, &cjs, &outside_js];

        let esm = es_module_files(&files, &project);

        assert_eq!(esm.len(), 2);
        assert!(esm.iter().any(|file| file.path.ends_with("module.js")));
        assert!(esm.iter().any(|file| file.path.ends_with("standalone.mjs")));
    }

    /// Build a `ProjectCtx` over `files`, then run `keep_jest_no_export` on a
    /// synthetic `jest-no-export` diagnostic pointing at `target_rel`. Returns
    /// whether the diagnostic is kept (`true`) or dropped as a shared fixture.
    fn keep_jest_no_export_on_project(files: &[(&str, &str)], target_rel: &str) -> bool {
        use crate::config::Config;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"t"}"#).unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
            let lang = crate::files::Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p,
                language: lang,
            });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::load(&refs, &Config::default());

        let target = dir.path().join(target_rel);
        let diag = crate::diagnostic::Diagnostic {
            path: std::sync::Arc::from(target.as_path()),
            line: 11,
            column: 1,
            rule_id: std::borrow::Cow::Borrowed("jest-no-export"),
            message: "Do not export from a test file".into(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        };
        keep_jest_no_export(&diag, &project)
    }

    #[test]
    fn jest_no_export_dropped_when_fixture_imported_by_sibling_spec_issue_4941() {
        // node-redis: FUNCTION_LOAD.spec.ts both tests FUNCTION_LOAD AND exports
        // a shared fixture (MATH_FUNCTION) + helper (loadMathFunction) consumed by
        // sibling spec files. The export is intentional, not a leak.
        let provider = "import { it } from 'vitest';\n\
            export const MATH_FUNCTION = { name: 'math' };\n\
            export function loadMathFunction(c) { return c.load(MATH_FUNCTION); }\n\
            describe('FUNCTION_LOAD', () => { it('loads', () => {}); });\n";
        let consumer = "import { MATH_FUNCTION, loadMathFunction } from './FUNCTION_LOAD.spec';\n\
            describe('FCALL', () => { it('calls', () => { loadMathFunction(MATH_FUNCTION); }); });\n";
        assert!(
            !keep_jest_no_export_on_project(
                &[
                    ("FUNCTION_LOAD.spec.ts", provider),
                    ("FCALL.spec.ts", consumer),
                ],
                "FUNCTION_LOAD.spec.ts",
            ),
            "export consumed by a sibling spec file is a shared fixture, not a leak"
        );
    }

    #[test]
    fn jest_no_export_kept_when_export_unconsumed() {
        // A real test file that exports something no other test file imports: the
        // export is a genuine leak and must still be flagged.
        let leaky = "import { it } from 'vitest';\n\
            export const helper = 1;\n\
            describe('A', () => { it('works', () => {}); });\n";
        let unrelated = "import { it } from 'vitest';\n\
            describe('B', () => { it('works', () => {}); });\n";
        assert!(
            keep_jest_no_export_on_project(
                &[("A.spec.ts", leaky), ("B.spec.ts", unrelated)],
                "A.spec.ts",
            ),
            "export imported by no other test file is still a leak"
        );
    }

    #[test]
    fn jest_no_export_kept_when_consumer_is_not_a_test_file() {
        // A production module importing a test file's export is a separate problem
        // (no-test-imports-in-prod). It must NOT silence jest-no-export.
        let provider = "import { it } from 'vitest';\n\
            export const helper = 1;\n\
            describe('A', () => { it('works', () => {}); });\n";
        let prod = "import { helper } from './A.spec';\nexport const x = helper + 1;\n";
        assert!(
            keep_jest_no_export_on_project(
                &[("A.spec.ts", provider), ("prod.ts", prod)],
                "A.spec.ts",
            ),
            "a non-test consumer must not exempt the export"
        );
    }
}
