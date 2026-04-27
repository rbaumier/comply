//! Persistent content-hash cache for the lint engine.
//!
//! Skips parse+lint for files whose contents haven't changed between runs.
//! The cache is a single bitcode-encoded file at
//! `{repo_root}/.comply/cache/v1.bitcode`.
//!
//! Lookup ladder per file:
//!   1. Fast path — `(mtime, size)` match the cached metadata. Return the
//!      cached diagnostics directly; no read, no parse.
//!   2. Hash path — `(mtime, size)` differ. The caller still has to read
//!      the file (it would have anyway), then `xxh3_128`-hashes the
//!      contents and compares to the cached hash. Match → return cached
//!      diagnostics, refresh the metadata so the next run takes the fast
//!      path. Mismatch → fall through to a full lint.
//!   3. Miss — no entry. Caller lints normally and `record`s the result.
//!
//! Failure policy: cache I/O is best-effort. A corrupted file, a
//! fingerprint mismatch (config / rule set / comply version changed),
//! or any read/write error logs to stderr and the lint continues with
//! a fresh, empty cache.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Severity};

/// Bumped whenever the on-disk schema for `CacheFile` changes shape in a way
/// that older deserializers would silently misread. Mismatch = discard.
const CACHE_FORMAT_VERSION: u32 = 1;

/// Top-level on-disk record. `entries` is keyed by path so each lookup
/// is O(1) regardless of project size.
#[derive(Serialize, Deserialize)]
struct CacheFile {
    cache_format_version: u32,
    fingerprint: u64,
    entries: FxHashMap<PathBuf, CacheEntry>,
}

/// One cached lint result. The `(mtime_secs, file_size)` pair is the
/// fast-path key; `content_hash` is the slow-path tiebreaker for editors
/// that touch mtime without changing bytes.
#[derive(Serialize, Deserialize, Clone)]
struct CacheEntry {
    content_hash: u128,
    mtime_secs: u64,
    file_size: u64,
    diagnostics: Vec<CachedDiagnostic>,
}

/// On-disk shape of a `Diagnostic`. Mirrors the public type field-for-field
/// minus the `skip_serializing_if` attribute that bitcode rejects, and
/// stores `path` / `rule_id` as plain owned strings so we don't have to
/// teach bitcode about `Arc<Path>` and `Cow<'static, str>`.
#[derive(Serialize, Deserialize, Clone)]
struct CachedDiagnostic {
    path: std::path::PathBuf,
    line: usize,
    column: usize,
    rule_id: String,
    message: String,
    severity: Severity,
    span: Option<(usize, usize)>,
}

impl CachedDiagnostic {
    fn from_diag(d: &Diagnostic) -> Self {
        Self {
            path: d.path.to_path_buf(),
            line: d.line,
            column: d.column,
            rule_id: d.rule_id.to_string(),
            message: d.message.clone(),
            severity: d.severity,
            span: d.span,
        }
    }

    fn into_diag(self) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(self.path),
            line: self.line,
            column: self.column,
            // Cached rule ids come back as owned strings — no &'static
            // available across runs. `Cow::Owned` is the right shape.
            rule_id: std::borrow::Cow::Owned(self.rule_id),
            message: self.message,
            severity: self.severity,
            span: self.span,
        }
    }
}

/// Result of a cache lookup. Diagnostics are materialized as owned
/// `Vec<Diagnostic>` because the cached storage form (`CachedDiagnostic`)
/// is not the public type — converting eagerly keeps the engine code
/// branch-free of cache details.
#[non_exhaustive]
pub enum LookupOutcome {
    /// `(mtime, size)` matched — diagnostics are guaranteed-fresh.
    Fresh(Vec<Diagnostic>),
    /// Metadata changed but a content hash is on file. Caller must read
    /// the source, hash it, and compare to `cached_hash`.
    NeedsHashCheck {
        cached_hash: u128,
        cached_diags: Vec<Diagnostic>,
    },
    /// No entry for this path. Caller must lint normally.
    Miss,
}

/// Persistent cache. Workers call `lookup` (immutable, lock-free) and
/// `record` (Mutex-guarded). At end-of-run, the main thread calls
/// `prune` then `flush`.
pub struct Cache {
    cache_path: PathBuf,
    fingerprint: u64,
    /// Snapshot loaded at `open` time. Read-only — workers can borrow
    /// `&[Diagnostic]` slices out of it without locking.
    loaded: FxHashMap<PathBuf, CacheEntry>,
    /// Entries written during this run. Workers push here under a
    /// mutex; `flush` drains it onto `loaded` before serializing.
    recorded: Mutex<FxHashMap<PathBuf, CacheEntry>>,
}

impl Cache {
    /// Open the cache at `{repo_root}/.comply/cache/v1.bitcode`.
    ///
    /// Returns an empty (but functional) cache on any of:
    ///   - file does not exist (first run)
    ///   - file is corrupted / fails to deserialize
    ///   - stored fingerprint disagrees with `fingerprint`
    ///
    /// Never fails — cache is always advisory.
    #[must_use]
    pub fn open(repo_root: &Path, fingerprint: u64) -> Self {
        let cache_path = repo_root.join(".comply").join("cache").join("v1.bitcode");
        let loaded = match std::fs::read(&cache_path) {
            Ok(bytes) => match bitcode::deserialize::<CacheFile>(&bytes) {
                Ok(file) => {
                    if file.cache_format_version == CACHE_FORMAT_VERSION
                        && file.fingerprint == fingerprint
                    {
                        file.entries
                    } else {
                        FxHashMap::default()
                    }
                }
                Err(_) => FxHashMap::default(),
            },
            Err(_) => FxHashMap::default(),
        };
        Self {
            cache_path,
            fingerprint,
            loaded,
            recorded: Mutex::new(FxHashMap::default()),
        }
    }

    /// Look up `path` against the snapshot loaded at `open` time.
    /// Lock-free — workers may call this concurrently.
    #[must_use]
    pub fn lookup(&self, path: &Path, meta: &std::fs::Metadata) -> LookupOutcome {
        let Some(entry) = self.loaded.get(path) else {
            return LookupOutcome::Miss;
        };
        let mtime = mtime_secs(meta);
        let size = meta.len();
        let diags: Vec<Diagnostic> = entry
            .diagnostics
            .iter()
            .cloned()
            .map(CachedDiagnostic::into_diag)
            .collect();
        if entry.mtime_secs == mtime && entry.file_size == size {
            return LookupOutcome::Fresh(diags);
        }
        LookupOutcome::NeedsHashCheck {
            cached_hash: entry.content_hash,
            cached_diags: diags,
        }
    }

    /// Insert or update an entry. Called by every worker after it has
    /// either confirmed a hash match or computed fresh diagnostics.
    pub fn record(
        &self,
        path: PathBuf,
        hash: u128,
        meta: &std::fs::Metadata,
        diagnostics: &[Diagnostic],
    ) {
        let entry = CacheEntry {
            content_hash: hash,
            mtime_secs: mtime_secs(meta),
            file_size: meta.len(),
            diagnostics: diagnostics.iter().map(CachedDiagnostic::from_diag).collect(),
        };
        // Poisoned mutex (a worker panicked) is recoverable — we don't
        // care about the previous holder's invariants because each
        // record is independent.
        let mut guard = self
            .recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.insert(path, entry);
    }

    /// Drop entries whose paths aren't in `present`. Run after a full
    /// lint pass to keep the cache from growing forever as files move
    /// or are deleted.
    pub fn prune(&self, present: &FxHashSet<&Path>) {
        let mut guard = self
            .recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.retain(|p, _| present.contains(p.as_path()));
        // `loaded` is read-only and never re-serialized directly — only
        // entries the workers re-recorded survive into the next run.
        // No work needed here.
    }

    /// Atomically write the cache to disk.
    ///
    /// Strategy: write to a tempfile in the same directory, then
    /// `persist()` (rename) onto the final path. Guarantees no torn
    /// reads: a concurrent run sees either the old file or the new
    /// one, never a half-written one.
    pub fn flush(self) -> anyhow::Result<()> {
        let Self {
            cache_path,
            fingerprint,
            loaded: _,
            recorded,
        } = self;

        let recorded = recorded.into_inner().unwrap_or_default();
        if recorded.is_empty() {
            // Nothing to persist — leave existing file alone.
            return Ok(());
        }

        let file = CacheFile {
            cache_format_version: CACHE_FORMAT_VERSION,
            fingerprint,
            entries: recorded,
        };
        let bytes = bitcode::serialize(&file)
            .map_err(|e| anyhow::anyhow!("cache serialize failed: {e}"))?;

        if let Some(dir) = cache_path.parent() {
            std::fs::create_dir_all(dir)?;
            let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
            std::io::Write::write_all(&mut tmp, &bytes)?;
            tmp.persist(&cache_path)
                .map_err(|e| anyhow::anyhow!("cache persist failed: {e}"))?;
        }
        Ok(())
    }
}

/// Hash a file's contents with xxh3-128. 128 bits is overkill for change
/// detection but cheap, and rules out collision-driven false negatives.
#[must_use]
pub fn hash_content(bytes: &[u8]) -> u128 {
    xxhash_rust::xxh3::xxh3_128(bytes)
}

/// Compute a fingerprint that invalidates the entire cache when:
///   - the comply binary version changes (rule semantics may have shifted)
///   - the set of registered rules changes (additions / removals / renames)
///   - the project's `comply.toml` changes (thresholds, severities, overrides)
///
/// Hashing the raw config file bytes is intentionally pessimistic:
/// even comment-only changes bust the cache. That's fine — config edits
/// are rare and a stale cache would be much worse than a one-off rebuild.
#[must_use]
pub fn compute_fingerprint(config: &Config, repo_root: &Path) -> u64 {
    use xxhash_rust::xxh3::xxh3_64;
    let _ = config;
    let mut bytes = Vec::with_capacity(512);
    bytes.extend_from_slice(env!("CARGO_PKG_VERSION").as_bytes());
    bytes.push(0);
    for rd in crate::rules::all_rule_defs() {
        bytes.extend_from_slice(rd.meta.id.as_bytes());
        bytes.push(0);
    }
    // Include the project's comply.toml verbatim if present. Walk up
    // from `repo_root` the same way `Config::load_from` does so a config
    // sitting in a parent directory still contributes.
    if let Some(cfg_path) = find_comply_toml(repo_root)
        && let Ok(cfg_bytes) = std::fs::read(&cfg_path)
    {
        bytes.push(1);
        bytes.extend_from_slice(&cfg_bytes);
    }
    xxh3_64(&bytes)
}

/// Walk up from `start` looking for `comply.toml`. Mirrors the lookup in
/// `config::find_comply_toml` (which is private).
fn find_comply_toml(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        let candidate = cur.join(crate::config::CONFIG_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Extract a unix-epoch seconds mtime. Files predating the epoch (or
/// platforms returning errors) get `0` — the slow path will still
/// catch genuine changes via the content hash.
fn mtime_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
