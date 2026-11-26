//! Translate per-clippy-lint thresholds in `comply.toml` into a
//! temporary `clippy.toml` that we point clippy at via `CLIPPY_CONF_DIR`.
//!
//! Why a separate file: clippy reads its own configuration (thresholds,
//! allow-lists, etc.) exclusively from a `clippy.toml` next to the
//! `Cargo.toml` it runs against. There's no command-line equivalent
//! for most of the keys. So if a user writes
//!
//!     [rules."clippy::too_many_lines"]
//!     threshold = 50
//!
//! in their `comply.toml`, comply has to materialize a `clippy.toml`
//! containing `too-many-lines-threshold = 50` somewhere clippy will
//! find it. We write it to a directory named after its content and pass
//! `CLIPPY_CONF_DIR=<that-dir>` to the cargo subprocess. Any other
//! clippy.toml key comes, copied verbatim, from the `[clippy]` table of
//! `comply.toml` or, without that table, from the project's own clippy.toml
//! (the one its plain `cargo clippy` reads). Both at once is refused.
//!
//! The mapping from `[rules."clippy::xxx"]` to `clippy.toml` keys is
//! the auto-generated `all_args::CLIPPY_THRESHOLD_LINTS` table.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use super::all_args::{ArgKind, CLIPPY_THRESHOLD_LINTS, ClippyArg};
use crate::config::Config;
use crate::files::SourceFile;
use crate::toolchain::ToolchainError;

/// Clippy's own file names, in its lookup order: `.clippy.toml` wins.
const CLIPPY_FILE_NAMES: [&str; 2] = [".clippy.toml", "clippy.toml"];

/// The generated `clippy.toml` of one workspace.
pub struct ClippyConf {
    /// The directory holding it, for `CLIPPY_CONF_DIR`.
    pub dir: PathBuf,
    copied: Option<CopiedFile>,
}

/// A project clippy file copied verbatim at the end of the generated one.
struct CopiedFile {
    path: PathBuf,
    /// Line of the generated file that holds the copy's first line.
    first_line: usize,
}

impl ClippyConf {
    /// Where `line`:`column` of the generated file was written from: that
    /// position in the copied project file, else comply.toml (`[clippy]` or a
    /// `[rules."clippy::…"]` threshold).
    #[must_use]
    pub fn origin_of(&self, line: usize, column: usize) -> String {
        match &self.copied {
            Some(copied) if line >= copied.first_line => format!(
                "{}:{}:{column}",
                copied.path.display(),
                line.saturating_sub(copied.first_line).saturating_add(1)
            ),
            _ => "comply.toml".to_owned(),
        }
    }
}

/// The clippy file plain `cargo clippy` reads for the workspace at `root`:
/// clippy takes the first directory holding one, from a package's directory
/// up to the filesystem root. comply hands one file to the whole workspace,
/// so a file clippy would read but comply cannot is an error: a second file
/// beside the one read, or one between a member owning `files` and `root`.
fn project_clippy_file(
    root: &Path,
    files: &[&SourceFile],
) -> Result<Option<PathBuf>, ToolchainError> {
    let found = root
        .ancestors()
        .map(clippy_files_in)
        .find(|found| !found.is_empty())
        .unwrap_or_default();
    if let [read, ignored, ..] = found.as_slice() {
        return Err(ToolchainError::Unusable {
            what: format!(
                "{} and {} both configure clippy: clippy reads the first and ignores the second.\n\
                 Merge them into one file",
                read.display(),
                ignored.display()
            ),
        });
    }
    let members: BTreeSet<PathBuf> = files
        .iter()
        .filter_map(|file| file.path.canonicalize().ok())
        .flat_map(|file| member_clippy_files(root, &file))
        .collect();
    if !members.is_empty() {
        return Err(ToolchainError::Unusable {
            what: format!(
                "plain clippy reads {} for a workspace member, but comply lints the workspace \
                 {} with one clippy configuration.\n\
                 Move the keys to [clippy] of comply.toml or to a clippy.toml at the workspace root, \
                 then delete the member file",
                members
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                root.display()
            ),
        });
    }
    Ok(found.into_iter().next())
}

fn clippy_files_in(dir: &Path) -> Vec<PathBuf> {
    CLIPPY_FILE_NAMES
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| path.is_file())
        .collect()
}

/// The clippy files below `root` that plain clippy reads for the package
/// owning `file`: those from the package's directory up to `root`, excluded.
fn member_clippy_files(root: &Path, file: &Path) -> Vec<PathBuf> {
    let dirs: Vec<&Path> = file
        .ancestors()
        .skip(1)
        .take_while(|dir| *dir != root && dir.starts_with(root))
        .collect();
    let Some(package) = dirs.iter().position(|dir| dir.join("Cargo.toml").is_file()) else {
        return Vec::new();
    };
    dirs[package..]
        .iter()
        .flat_map(|dir| clippy_files_in(dir))
        .collect()
}

/// The keys copied verbatim into the generated `clippy.toml`, and where they
/// come from: `[clippy]` of comply.toml, else the project's own clippy file.
/// A project file beside a `[clippy]` table, even an empty one, is refused.
enum Base {
    Table(toml::Table),
    File {
        path: PathBuf,
        text: String,
        keys: toml::Table,
    },
}

impl Base {
    fn keys(&self) -> &toml::Table {
        match self {
            Self::Table(keys) | Self::File { keys, .. } => keys,
        }
    }

    fn source(&self) -> String {
        match self {
            Self::Table(_) => "[clippy] of comply.toml".to_owned(),
            Self::File { path, .. } => path.display().to_string(),
        }
    }
}

fn base(config: &Config, root: &Path, files: &[&SourceFile]) -> Result<Base, ToolchainError> {
    let passthrough = config.clippy_passthrough();
    let Some(path) = project_clippy_file(root, files)? else {
        return Ok(Base::Table(passthrough.cloned().unwrap_or_default()));
    };
    let parsed = std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| match text.parse::<toml::Table>() {
            Ok(keys) => Ok((text, keys)),
            Err(e) => Err(e.to_string()),
        });
    let what = match (parsed, passthrough.is_some()) {
        (Ok((text, keys)), false) => return Ok(Base::File { path, text, keys }),
        (Ok((_, keys)), true) => format!(
            "{} is not read: [clippy] of comply.toml is the single source of clippy's config.\n\
             Move its keys ({}) under [clippy] in comply.toml, then delete the file",
            path.display(),
            keys.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
        (Err(e), false) => format!("{} cannot be read or parsed: {e}", path.display()),
        (Err(e), true) => format!(
            "{} cannot be read or parsed: {e}\n\
             Move its content under [clippy] in comply.toml, then delete the file",
            path.display()
        ),
    };
    Err(ToolchainError::Unusable { what })
}

/// Build the `clippy.toml` for the workspace at `root`, linted for `files`,
/// from thresholds and the keys of `base`. A key set both ways is an error:
/// every threshold here was set in comply.toml, `defaults.toml` declares none.
/// A project file is copied as is after the thresholds, so a line clippy
/// rejects maps back to its own line.
fn render_clippy_toml(
    config: &Config,
    root: &Path,
    files: &[&SourceFile],
) -> Result<(String, Option<CopiedFile>), ToolchainError> {
    let base = base(config, root, files)?;
    let mut lines: Vec<String> = Vec::new();

    for (rule_id, rule) in config.iter_rules() {
        // Only `clippy::*` rules are eligible for the threshold mapping;
        // tree-sitter and oxlint rules have nothing to do with clippy's
        // own configuration file.
        if !rule_id.starts_with("clippy::") {
            continue;
        }
        let Some(arg) = lookup_arg(rule_id) else {
            continue;
        };
        let value_key = match arg.kind {
            ArgKind::Int | ArgKind::String => "threshold",
            ArgKind::Array => "values",
        };
        let Some(value) = rule.extra.get(value_key) else {
            continue;
        };
        if base.keys().contains_key(arg.clippy_toml_key) {
            return Err(ToolchainError::Unusable {
                what: format!(
                    "`{}` is set both by [rules.\"{rule_id}\"] and in {}; keep one",
                    arg.clippy_toml_key,
                    base.source()
                ),
            });
        }
        if let Some(line) = encode_arg(arg, value) {
            lines.push(line);
        }
    }

    let header = "# generated by comply — do not edit by hand.\n\
                  # This file is materialized in a content-addressed dir and\n\
                  # passed to `cargo clippy` via the CLIPPY_CONF_DIR env var.\n";
    let head = format!("{header}{}\n", lines.join("\n"));
    match base {
        Base::Table(keys) => {
            let text = toml::to_string(&keys).map_err(|e| ToolchainError::Unusable {
                what: format!("[clippy] of comply.toml cannot be rendered as clippy.toml: {e}"),
            })?;
            Ok((format!("{head}{text}"), None))
        }
        Base::File { path, text, .. } => {
            let marker = format!("# keys below are copied from {}\n", path.display());
            let first_line = head.matches('\n').count().saturating_add(2);
            Ok((
                format!("{head}{marker}{text}"),
                Some(CopiedFile { path, first_line }),
            ))
        }
    }
}

/// Materialize the rendered `clippy.toml` for the workspace at `root`, linted
/// for `files`, and return its directory, for `CLIPPY_CONF_DIR`.
///
/// Clippy records `CLIPPY_CONF_DIR` and the file's mtime in each crate's
/// dep-info: a fresh temp dir per run made cargo re-lint every crate of the
/// workspace on every comply run. The directory is named after the content
/// hash and never deleted, so an unchanged config keeps the same path and
/// mtime, and clippy stays incremental.
pub fn materialize(config: &Config, root: &Path, files: &[&SourceFile]) -> Result<ClippyConf> {
    let (body, copied) = render_clippy_toml(config, root, files)?;
    let mut hasher = rustc_hash::FxHasher::default();
    body.hash(&mut hasher);
    let dir = user_cache_dir()
        .join("comply/clippy-conf")
        .join(format!("{:016x}", hasher.finish()));
    let path = dir.join("clippy.toml");
    if path.is_file() {
        return Ok(ClippyConf { dir, copied });
    }
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create {} for clippy.toml", dir.display()))?;
    // Write then rename: two comply runs in parallel worktrees never read a
    // half-written file.
    let tmp = tempfile::NamedTempFile::new_in(&dir).context("failed to create clippy.toml")?;
    std::fs::write(tmp.path(), body).context("failed to write clippy.toml")?;
    tmp.persist(&path)
        .with_context(|| format!("failed to move clippy.toml into {}", dir.display()))?;
    Ok(ClippyConf { dir, copied })
}

/// A per-user cache, not the temp dir: macOS purges `$TMPDIR` after a few idle
/// days (one full re-lint each time), and a shared `/tmp` path is predictable
/// to other users.
fn user_cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return dir.into();
    }
    match std::env::var_os("HOME") {
        Some(home) if cfg!(target_os = "macos") => Path::new(&home).join("Library/Caches"),
        Some(home) => Path::new(&home).join(".cache"),
        None => std::env::temp_dir(),
    }
}

/// Cargo expects `CLIPPY_CONF_DIR` to point to a directory containing
/// `clippy.toml`, not to the file itself.
#[must_use]
pub fn env_var_for_dir(dir: &Path) -> (String, String) {
    ("CLIPPY_CONF_DIR".to_string(), dir.display().to_string())
}

fn lookup_arg(rule_id: &str) -> Option<&'static ClippyArg> {
    CLIPPY_THRESHOLD_LINTS.iter().find(|a| a.lint == rule_id)
}

/// Render one TOML key/value line for a clippy threshold. Returns
/// `None` when the user's value doesn't fit the expected shape (e.g.
/// they wrote a string for an int knob).
fn encode_arg(arg: &ClippyArg, value: &toml::Value) -> Option<String> {
    let key = arg.clippy_toml_key;
    match arg.kind {
        ArgKind::Int => {
            let n = value.as_integer()?;
            Some(format!("{key} = {n}"))
        }
        ArgKind::String => {
            let s = value.as_str()?;
            Some(format!("{key} = \"{}\"", escape_toml_string(s)))
        }
        ArgKind::Array => {
            let arr = value.as_array()?;
            let items: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| format!("\"{}\"", escape_toml_string(s)))
                .collect();
            Some(format!("{key} = [{}]", items.join(", ")))
        }
    }
}

fn escape_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::fs;
    use tempfile::TempDir;

    fn config_with(toml_text: &str) -> Config {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(&cfg_path, toml_text).unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        // Leak the tempdir so the test config keeps the file alive —
        // we only need the parsed Config, not the original on-disk file.
        std::mem::forget(tmp);
        cfg
    }

    /// Renders for a workspace without a clippy file of its own.
    fn render(cfg: &Config) -> Result<String, ToolchainError> {
        render_project(cfg, cfg.dir().unwrap())
    }

    /// Renders for a workspace at `root` linted without a member file.
    fn render_project(cfg: &Config, root: &Path) -> Result<String, ToolchainError> {
        render_clippy_toml(cfg, root, &[]).map(|(body, _)| body)
    }

    #[test]
    fn renders_int_threshold_for_too_many_lines() {
        let cfg = config_with(
            r#"
            [rules."clippy::too_many_lines"]
            threshold = 50
            "#,
        );
        let body = render(&cfg).unwrap();
        assert!(body.contains("too-many-lines-threshold = 50"));
    }

    #[test]
    fn renders_array_threshold_for_disallowed_names() {
        let cfg = config_with(
            r#"
            [rules."clippy::disallowed_names"]
            values = ["foo", "bar", "baz"]
            "#,
        );
        let body = render(&cfg).unwrap();
        assert!(body.contains("disallowed-names = [\"foo\", \"bar\", \"baz\"]"));
    }

    #[test]
    fn renders_empty_table_when_no_clippy_key_present() {
        let cfg = config_with(
            r#"
            [rules.max-function-lines]
            max = 60
            "#,
        );
        let body = render(&cfg).unwrap();
        assert!(body.parse::<toml::Table>().unwrap().is_empty(), "{body}");
    }

    #[test]
    fn ignores_unknown_clippy_lint_keys() {
        let cfg = config_with(
            r#"
            [rules."clippy::needless_borrow"]
            threshold = 99
            "#,
        );
        // `needless_borrow` has no threshold in our table — skip it.
        let body = render(&cfg).unwrap();
        assert!(body.parse::<toml::Table>().unwrap().is_empty(), "{body}");
    }

    #[test]
    fn renders_passthrough_disallowed_methods_with_reason() {
        let cfg = config_with(
            r#"
            [clippy]
            disallowed-methods = [{ path = "std::env::var", reason = "read \"Config\"" }]
            "#,
        );
        let body = render(&cfg).unwrap();
        let rendered: toml::Table = body.parse().unwrap();
        let entry = &rendered["disallowed-methods"][0];
        assert_eq!(entry["path"].as_str(), Some("std::env::var"));
        assert_eq!(entry["reason"].as_str(), Some("read \"Config\""));
    }

    #[test]
    fn rejects_key_set_as_threshold_and_in_passthrough() {
        let cfg = config_with(
            r#"
            [rules."clippy::too_many_lines"]
            threshold = 50

            [clippy]
            too-many-lines-threshold = 80
            "#,
        );
        let err = render(&cfg).unwrap_err();
        assert!(
            err.to_string().contains("too-many-lines-threshold"),
            "{err}"
        );
    }

    #[test]
    fn threshold_and_passthrough_render_side_by_side() {
        let cfg = config_with(
            r#"
            [rules."clippy::too_many_lines"]
            threshold = 50

            [clippy]
            disallowed-methods = [{ path = "std::env::var", reason = "read Config" }]
            "#,
        );
        let body = render(&cfg).unwrap();
        let rendered: toml::Table = body.parse().unwrap();
        assert_eq!(rendered["too-many-lines-threshold"].as_integer(), Some(50));
        assert!(rendered["disallowed-methods"].is_array(), "{body}");
    }

    #[test]
    fn project_clippy_toml_beside_clippy_table_is_refused_with_its_keys() {
        let cfg = config_with("[clippy]\nmsrv = \"1.70\"\n");
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("clippy.toml"),
            "too-many-lines-threshold = 80\n\
             disallowed-methods = [{ path = \"reqwest::get\", reason = \"use Http\" }]\n",
        )
        .unwrap();
        let err = render_project(&cfg, tmp.path()).unwrap_err().to_string();
        assert!(err.contains("clippy.toml"), "{err}");
        assert!(err.contains("disallowed-methods"), "{err}");
        assert!(err.contains("too-many-lines-threshold"), "{err}");
    }

    #[test]
    fn project_clippy_toml_keys_render_beside_thresholds() {
        let cfg = config_with("[rules.\"clippy::too_many_lines\"]\nthreshold = 50\n");
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join(".clippy.toml"),
            "disallowed-methods = [{ path = \"reqwest::get\", reason = \"use Http\" }]\n",
        )
        .unwrap();
        let body = render_project(&cfg, tmp.path()).unwrap();
        let rendered: toml::Table = body.parse().unwrap();
        assert_eq!(rendered["too-many-lines-threshold"].as_integer(), Some(50));
        assert_eq!(
            rendered["disallowed-methods"][0]["reason"].as_str(),
            Some("use Http")
        );
    }

    #[test]
    fn unparseable_project_clippy_toml_is_refused_without_a_key_list() {
        let cfg = config_with("");
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".clippy.toml"), "msrv = \n").unwrap();
        let err = render_project(&cfg, tmp.path()).unwrap_err().to_string();
        assert!(
            err.contains(".clippy.toml cannot be read or parsed"),
            "{err}"
        );
        assert!(!err.contains("Move its keys"), "{err}");
    }

    #[test]
    fn copied_project_line_maps_back_to_its_own_line() {
        let cfg = config_with("[rules.\"clippy::too_many_lines\"]\nthreshold = 50\n");
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join(".clippy.toml");
        fs::write(
            &project,
            "msrv = \"1.70\"\ntoo-many-arguments-treshold = 2\n",
        )
        .unwrap();
        let conf = materialize(&cfg, tmp.path(), &[]).unwrap();
        let body = fs::read_to_string(conf.dir.join("clippy.toml")).unwrap();
        let line_of = |prefix: &str| body.lines().position(|l| l.starts_with(prefix)).unwrap() + 1;

        assert_eq!(
            conf.origin_of(line_of("too-many-arguments-treshold"), 1),
            format!("{}:2:1", project.display())
        );
        assert_eq!(
            conf.origin_of(line_of("too-many-lines-threshold"), 1),
            "comply.toml"
        );
    }

    // Regression: a fresh temp dir per run re-linted the whole workspace on every run.
    #[test]
    fn same_config_keeps_the_same_dir_and_mtime() {
        let cfg = config_with("[clippy]\nmsrv = \"1.70\"\n");
        let first = materialize(&cfg, cfg.dir().unwrap(), &[]).unwrap().dir;
        let mtime = fs::metadata(first.join("clippy.toml"))
            .unwrap()
            .modified()
            .unwrap();
        let second = materialize(&cfg, cfg.dir().unwrap(), &[]).unwrap().dir;
        assert_eq!(first, second);
        assert_eq!(
            fs::metadata(second.join("clippy.toml"))
                .unwrap()
                .modified()
                .unwrap(),
            mtime
        );

        let other_cfg = config_with("[clippy]\nmsrv = \"1.71\"\n");
        let other = materialize(&other_cfg, other_cfg.dir().unwrap(), &[])
            .unwrap()
            .dir;
        assert_ne!(first, other);
        assert!(
            fs::read_to_string(other.join("clippy.toml"))
                .unwrap()
                .contains("1.71")
        );
    }
}
