//! Project configuration loaded from `comply.toml`.
//!
//! Lookup order at startup:
//!   1. Walk up from the current working directory looking for the
//!      nearest `comply.toml`.
//!   2. If found, parse it and merge the user's overrides on top of
//!      the hardcoded defaults from `defaults::build_default_config`.
//!   3. If not found, use defaults verbatim.
//!
//! Public API for the rest of the codebase:
//!   - `Config::load_from_cwd()` — entry point used by `main`
//!   - `Config::default()` — defaults only, used by tests
//!   - `Config::is_rule_enabled(rule_id, file_path)` — combine global
//!     `disabled = true` with per-glob `disable = [...]` overrides
//!   - `Config::severity_for(rule_id)` — global severity override
//!   - `Config::threshold(rule_id, key)` — typed threshold accessor
//!     used by every rule that has a knob (panics if the key is not
//!     declared in `src/config/defaults.toml`)
//!   - `Config::print_default_toml()` — pretty-print the defaults so
//!     `comply config init` can dump them to disk

mod defaults;
mod schema;

pub use schema::{ComplyToml, RuleConfig};

/// Process-wide default config used by `CheckCtx::for_test` so unit
/// tests don't have to construct one. Initialized lazily on first use.
/// Test-only — production code threads a real `Config` through `engine`.
#[cfg(test)]
pub fn default_static_config() -> &'static Config {
    use std::sync::OnceLock;
    static DEFAULT: OnceLock<Config> = OnceLock::new();
    DEFAULT.get_or_init(Config::default)
}

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

use crate::diagnostic::Severity;
use crate::files::Language;

type LangConfigMap = FxHashMap<String, FxHashMap<Language, FxHashMap<String, toml::Value>>>;

/// File name we look for. Always lowercase, never `.comply.toml`
/// (no dot prefix) so it shows up in default file listings.
pub const CONFIG_FILE_NAME: &str = "comply.toml";

/// Resolved configuration: defaults merged with the user's `comply.toml`,
/// plus a precompiled glob matcher for the per-path overrides.
#[derive(Debug)]
pub struct Config {
    raw: ComplyToml,
    /// Compiled globs in the order they appear in the TOML. Each
    /// `disable_lists[i]` is the rule-id list to silence when the
    /// matcher's i-th glob fires for a path.
    glob_matcher: GlobSet,
    disable_lists: Vec<Vec<String>>,
    /// Per-language config extracted from qualified rule keys like
    /// `[rules."id-length.ts"]` or `[rules."id-length.{ts,rs}"]`.
    /// Keyed by `base_rule_id -> Language -> key -> value`.
    lang_config: LangConfigMap,
}

impl Config {
    /// Defaults-only config — used by tests and as the base layer
    /// when no `comply.toml` is found in the project tree.
    pub fn default() -> Self {
        // defaults::build_default_config() is statically known to compile;
        // a panic here means a programmer bug in the defaults table, not a
        // runtime condition.
        Self::from_raw(defaults::build_default_config()) // comply-ignore: rust-no-unwrap — defaults are static.
            .expect("default config must compile")
    }

    /// Walk up from `start_dir` looking for the nearest `comply.toml`.
    /// If one is found, layer the user's overrides on top of the
    /// defaults; otherwise return the defaults verbatim.
    pub fn load_from(start_dir: &Path) -> Result<Self> {
        let Some(path) = find_comply_toml(start_dir) else {
            return Ok(Self::default());
        };
        let user_text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let user: ComplyToml = toml::from_str(&user_text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let merged = merge(defaults::build_default_config(), user);
        Self::from_raw(merged)
    }

    /// Return the embedded `defaults.toml` verbatim — used by
    /// `comply config init` to seed a project's `comply.toml`.
    /// Returning the raw file preserves comments, section ordering,
    /// and the commented-out examples (e.g. `id-length` exceptions)
    /// that a round-trip through `toml::to_string_pretty` would erase.
    #[must_use]
    pub fn print_default_toml() -> String {
        defaults::default_toml_text().to_string()
    }

    #[must_use]
    pub fn theme(&self) -> Option<&str> {
        self.raw.theme.as_deref()
    }

    /// User-configured additional graph roots (globs relative to project root).
    #[must_use]
    pub fn entrypoints(&self) -> &[String] {
        &self.raw.entrypoints
    }

    /// True if `rule_id` is enabled for `file_path`. Combines:
    ///   - global `[rules.<id>] disabled = true` (kills the rule everywhere)
    ///   - per-glob `[overrides."<g>"] disable = [<id>]` (kills it for matches)
    #[must_use]
    pub fn is_rule_enabled(&self, rule_id: &str, file_path: &Path) -> bool {
        if let Some(rule) = self.raw.rules.get(rule_id)
            && rule.disabled == Some(true)
        {
            return false;
        }
        // No per-path overrides → the global `disabled` check above is the only
        // gate. Skip the path normalization + glob match (both allocate) that
        // would otherwise run once per (rule × file) on the engine hot path.
        if self.glob_matcher.is_empty() {
            return true;
        }
        // Override globs are relative (e.g. `src/foo/**`). File paths arrive
        // in two forms:
        //   - `./src/foo.ts`  from the engine walker  → strip leading `./`
        //   - `/abs/path/src/foo.ts` from cross-file rules (unused-file)
        //     that store canonicalized absolute paths in the import index
        //     → relativize against CWD (comply is always invoked from the
        //     project root, which is where comply.toml and the override
        //     globs are anchored).
        let abs_relative: Option<PathBuf> = if file_path.is_absolute() {
            std::env::current_dir()
                .ok()
                .and_then(|cwd| file_path.strip_prefix(&cwd).ok().map(|r| r.to_path_buf()))
        } else {
            None
        };
        let relative: &Path = abs_relative
            .as_deref()
            .unwrap_or_else(|| file_path.strip_prefix("./").unwrap_or(file_path));
        for idx in self.glob_matcher.matches(relative) {
            if self.disable_lists[idx].iter().any(|d| d == rule_id) {
                return false;
            }
        }
        true
    }

    /// Override severity for a rule, or `None` if the user didn't set one.
    #[must_use]
    pub fn severity_for(&self, rule_id: &str) -> Option<Severity> {
        self.raw
            .rules
            .get(rule_id)
            .and_then(|r| r.severity)
            .map(Into::into)
    }

    /// Iterate over every `[rules.<id>]` block the user (or the
    /// defaults) has configured. Returns `(rule_id, &RuleConfig)`
    /// pairs. Used by the clippy module to discover which lints to
    /// enable explicitly and which thresholds to write into the
    /// generated `clippy.toml`.
    pub fn iter_rules(&self) -> impl Iterator<Item = (&str, &RuleConfig)> {
        self.raw.rules.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Read a numeric threshold for `rule_id`. Panics if the key is
    /// missing from the merged config (defaults + user) — by design,
    /// `src/config/defaults.toml` is the single source of truth and
    /// any rule that asks for a knob must have declared it there.
    /// A fallback argument would silently diverge from the TOML the
    /// day one side gets updated and the other doesn't.
    #[must_use]
    pub fn threshold(&self, rule_id: &str, key: &str, lang: Language) -> usize {
        let value = self.extra_value(rule_id, key, lang);
        let Some(n) = value.as_integer().and_then(|n| usize::try_from(n).ok()) else {
            panic!(
                "config key `[rules.\"{rule_id}\"] {key}` must be a \
                 non-negative integer, got {value:?}"
            );
        };
        n
    }

    /// Read a float-valued threshold for `rule_id`. Used by rules whose
    /// knob is a fraction / probability (overlap ratios, confidence
    /// thresholds) rather than a count. Integer values in TOML are
    /// accepted and coerced (so `min_ratio = 1` behaves as `1.0`).
    /// Panics with a clear message when the key is absent — same
    /// contract as `threshold`: defaults.toml is authoritative.
    #[must_use]
    pub fn float(&self, rule_id: &str, key: &str, lang: Language) -> f64 {
        let value = self.extra_value(rule_id, key, lang);
        if let Some(f) = value.as_float() {
            return f;
        }
        if let Some(n) = value.as_integer() {
            return n as f64;
        }
        panic!("config key `[rules.\"{rule_id}\"] {key}` must be a number, got {value:?}");
    }

    /// Boolean config flag for `rule_id`. Panics if the key is absent from
    /// the merged config — add it to `src/config/defaults.toml` as the
    /// authoritative default.
    #[must_use]
    pub fn bool_flag(&self, rule_id: &str, key: &str, lang: Language) -> bool {
        let value = self.extra_value(rule_id, key, lang);
        let Some(b) = value.as_bool() else {
            panic!("config key `[rules.\"{rule_id}\"] {key}` must be a boolean, got {value:?}");
        };
        b
    }

    /// Shared lookup for `threshold` / `float`. Panics with a
    /// uniform "missing key" message so the two public APIs don't
    /// duplicate the same boilerplate.
    fn extra_value(&self, rule_id: &str, key: &str, lang: Language) -> &toml::Value {
        if let Some(value) = self
            .lang_config
            .get(rule_id)
            .and_then(|by_lang| by_lang.get(&lang))
            .and_then(|extras| extras.get(key))
        {
            return value;
        }
        let Some(value) = self.raw.rules.get(rule_id).and_then(|r| r.extra.get(key)) else {
            panic!(
                "config key `[rules.\"{rule_id}\"] {key}` is missing — \
                 add it to `src/config/defaults.toml`"
            );
        };
        value
    }

    /// Read a list of strings from `[rules.<rule_id>] <key> = [...]`.
    /// Non-string entries and absent keys collapse to an empty vec, so
    /// the caller can treat "not configured" and "configured empty" the
    /// same way. Used by rules that match against a user-configured
    /// pattern list (e.g. `ts-no-restricted-imports`).
    #[must_use]
    pub fn string_list(&self, rule_id: &str, key: &str, lang: Language) -> Vec<String> {
        let value = self
            .lang_config
            .get(rule_id)
            .and_then(|by_lang| by_lang.get(&lang))
            .and_then(|extras| extras.get(key))
            .or_else(|| {
                self.raw
                    .rules
                    .get(rule_id)
                    .and_then(|r| r.extra.get(key))
            });
        value
            .and_then(toml::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn from_raw(mut raw: ComplyToml) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        let mut disable_lists: Vec<Vec<String>> = Vec::new();
        for (pattern, override_cfg) in &raw.overrides {
            // Normalize keys the same way as the paths they match against:
            // a leading `./` is dropped so both `./src/**` and `src/**`
            // resolve to the same glob.
            let normalized = pattern.strip_prefix("./").unwrap_or(pattern);
            let glob = Glob::new(normalized)
                .with_context(|| format!("invalid glob in [overrides.\"{pattern}\"]"))?;
            builder.add(glob);
            disable_lists.push(override_cfg.disable.clone());
        }
        let glob_matcher = builder
            .build()
            .context("failed to compile [overrides] globs")?;

        let lang_config = build_lang_config(&mut raw)?;

        Ok(Self {
            raw,
            glob_matcher,
            disable_lists,
            lang_config,
        })
    }
}

const ALL_LANGUAGES: [Language; 12] = [
    Language::TypeScript,
    Language::Tsx,
    Language::JavaScript,
    Language::Rust,
    Language::Vue,
    Language::Toml,
    Language::Json,
    Language::Css,
    Language::Yaml,
    Language::Dockerfile,
    Language::Sql,
    Language::GraphQl,
];

/// Extract language-qualified rule keys (e.g. `"id-length.ts"`,
/// `"id-length.{ts,rs}"`, `"id-length.ts*"`) from `raw.rules` and
/// return them as a lookup table keyed by `(base_rule_id, Language)`.
/// Matched keys are removed from `raw.rules` so they don't pollute
/// `iter_rules()`.
fn build_lang_config(raw: &mut ComplyToml) -> Result<LangConfigMap> {
    let mut lang_config: LangConfigMap = FxHashMap::default();
    let mut qualified_keys: Vec<String> = Vec::new();

    for (key, rule_cfg) in raw.rules.iter() {
        let Some(dot_pos) = key.rfind('.') else {
            continue;
        };
        let base_id = &key[..dot_pos];
        let suffix = &key[dot_pos + 1..];
        let glob = Glob::new(suffix)
            .with_context(|| format!("invalid lang glob in [rules.\"{key}\"]"))?;
        let matcher = glob.compile_matcher();
        let mut matched_any = false;
        for &lang in &ALL_LANGUAGES {
            if matcher.is_match(lang.config_suffix()) {
                lang_config
                    .entry(base_id.to_string())
                    .or_default()
                    .entry(lang)
                    .or_default()
                    .extend(rule_cfg.extra.iter().map(|(k, v)| (k.clone(), v.clone())));
                matched_any = true;
            }
        }
        if matched_any {
            qualified_keys.push(key.clone());
        }
    }

    for key in &qualified_keys {
        raw.rules.remove(key);
    }

    Ok(lang_config)
}

/// Walk up from `start` looking for `comply.toml`. Returns the absolute
/// path to the file if one is found, or `None` if we hit the filesystem
/// root without finding one.
fn find_comply_toml(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        let candidate = cur.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Layer `user` on top of `base`. Per-rule overrides merge field-by-field:
/// the user's `disabled`, `severity`, and individual `extra` knobs win,
/// while everything they didn't touch falls back to the default.
/// Per-path overrides replace whatever was at that glob.
fn merge(mut base: ComplyToml, user: ComplyToml) -> ComplyToml {
    if user.theme.is_some() {
        base.theme = user.theme;
    }
    if !user.entrypoints.is_empty() {
        base.entrypoints = user.entrypoints;
    }
    for (rule_id, user_rule) in user.rules {
        let entry = base.rules.entry(rule_id).or_default();
        if let Some(d) = user_rule.disabled {
            entry.disabled = Some(d);
        }
        if let Some(e) = user_rule.enabled {
            entry.enabled = Some(e);
        }
        if let Some(s) = user_rule.severity {
            entry.severity = Some(s);
        }
        for (k, v) in user_rule.extra {
            entry.extra.insert(k, v);
        }
    }
    for (glob, override_cfg) in user.overrides {
        base.overrides.insert(glob, override_cfg);
    }
    base
}

// Workaround: serde serialization needs ComplyToml to be Serialize for
// `print_default_toml`. Wire it up here rather than in schema.rs so the
// public schema struct stays minimal.
mod serialize_impl {
    use super::schema::{ComplyToml, OverrideConfig, RuleConfig, SeverityToml};
    use serde::ser::SerializeStruct;
    use serde::{Serialize, Serializer};

    impl Serialize for ComplyToml {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut state = s.serialize_struct("ComplyToml", 3)?;
            state.serialize_field("entrypoints", &self.entrypoints)?;
            state.serialize_field("rules", &self.rules)?;
            state.serialize_field("overrides", &self.overrides)?;
            state.end()
        }
    }

    impl Serialize for RuleConfig {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            // Inline `extra` into the rule's table so it reads as
            // `[rules.foo] max = 30` instead of `[rules.foo.extra]`.
            use serde::ser::SerializeMap;
            let mut len = self.extra.len();
            if self.disabled.is_some() {
                len += 1;
            }
            if self.severity.is_some() {
                len += 1;
            }
            let mut map = s.serialize_map(Some(len))?;
            if let Some(d) = self.disabled {
                map.serialize_entry("disabled", &d)?;
            }
            if let Some(sev) = self.severity {
                map.serialize_entry("severity", &sev)?;
            }
            for (k, v) in &self.extra {
                map.serialize_entry(k, v)?;
            }
            map.end()
        }
    }

    impl Serialize for OverrideConfig {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut state = s.serialize_struct("OverrideConfig", 1)?;
            state.serialize_field("disable", &self.disable)?;
            state.end()
        }
    }

    impl Serialize for SeverityToml {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            match self {
                SeverityToml::Warning => s.serialize_str("warning"),
                SeverityToml::Error => s.serialize_str("error"),
            }
        }
    }
}

#[cfg(test)]
impl Config {
    /// Build a default `Config` with the given entrypoints globs set.
    /// Used by `dead-export` and `unused-file` regression tests.
    pub fn with_entrypoints(globs: Vec<String>) -> Self {
        let mut cfg = Self::default();
        cfg.raw.entrypoints = globs;
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn default_config_returns_known_thresholds() {
        let cfg = Config::default();
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::TypeScript), 30);
        assert_eq!(cfg.threshold("max-file-lines", "max", Language::TypeScript), 200);
    }

    #[test]
    #[should_panic(expected = "is missing")]
    fn threshold_panics_when_key_missing() {
        let cfg = Config::default();
        let _ = cfg.threshold("does-not-exist", "max", Language::TypeScript);
    }

    #[test]
    fn string_list_returns_empty_when_unconfigured() {
        let cfg = Config::default();
        assert!(cfg.string_list("does-not-exist", "patterns", Language::TypeScript).is_empty());
    }

    #[test]
    fn string_list_reads_user_array() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [rules.ts-no-restricted-imports]
            patterns = ["@banned/*", "legacy"]
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        let list = cfg.string_list("ts-no-restricted-imports", "patterns", Language::TypeScript);
        assert_eq!(list, vec!["@banned/*", "legacy"]);
    }

    #[test]
    fn user_config_overrides_threshold() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [rules.max-function-lines]
            max = 80
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::TypeScript), 80);
        // Other defaults still intact.
        assert_eq!(cfg.threshold("max-file-lines", "max", Language::TypeScript), 200);
    }

    #[test]
    fn disabled_rule_is_filtered_globally() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [rules.max-function-lines]
            disabled = true
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert!(!cfg.is_rule_enabled("max-function-lines", Path::new("src/foo.rs")));
        assert!(cfg.is_rule_enabled("max-file-lines", Path::new("src/foo.rs")));
    }

    #[test]
    fn glob_override_disables_rule_only_for_matching_paths() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [overrides."tests/**"]
            disable = ["rust-no-unwrap"]
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert!(!cfg.is_rule_enabled("rust-no-unwrap", Path::new("tests/foo.rs")));
        assert!(cfg.is_rule_enabled("rust-no-unwrap", Path::new("src/foo.rs")));
    }

    #[test]
    fn override_matches_regardless_of_dot_slash_prefix() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [overrides."src/api/errors/from-database.ts"]
            disable = ["intermediate-variables"]

            [overrides."src/api/test-helpers/**"]
            disable = ["rust-no-unwrap"]
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        // Engine reports paths with a `./` prefix; the unprefixed override
        // key must still match (the issue's reproducer).
        assert!(!cfg.is_rule_enabled(
            "intermediate-variables",
            Path::new("./src/api/errors/from-database.ts")
        ));
        assert!(!cfg.is_rule_enabled(
            "rust-no-unwrap",
            Path::new("./src/api/test-helpers/setup-env.ts")
        ));
        // The previously-working prefixed form keeps matching too.
        assert!(!cfg.is_rule_enabled(
            "intermediate-variables",
            Path::new("src/api/errors/from-database.ts")
        ));
    }

    #[test]
    fn dot_slash_prefixed_override_key_still_matches() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [overrides."./src/api/errors/from-database.ts"]
            disable = ["intermediate-variables"]
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert!(!cfg.is_rule_enabled(
            "intermediate-variables",
            Path::new("./src/api/errors/from-database.ts")
        ));
        assert!(!cfg.is_rule_enabled(
            "intermediate-variables",
            Path::new("src/api/errors/from-database.ts")
        ));
    }

    // Regression for #496: unused-file stores canonical absolute paths in the
    // import index. Override globs are relative. is_rule_enabled must relativize
    // absolute paths against CWD so the glob still fires.
    #[test]
    fn absolute_path_override_matches_relative_glob() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("comply.toml"),
            r#"
            [overrides."src/app/components/data-table/**"]
            disable = ["unused-file"]
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();

        // Simulate what unused-file emits: an absolute canonical path.
        // We compute one by canonicalizing a known path under CWD. We use
        // the test tmp dir itself to build a plausible absolute path.
        let cwd = std::env::current_dir().unwrap();
        let abs_path = cwd.join("src/app/components/data-table/body.tsx");
        assert!(
            !cfg.is_rule_enabled("unused-file", &abs_path),
            "absolute path inside overridden glob must disable the rule"
        );

        // A file outside the override must still be enabled.
        let abs_other = cwd.join("src/app/other/file.tsx");
        assert!(
            cfg.is_rule_enabled("unused-file", &abs_other),
            "absolute path outside overridden glob must keep the rule enabled"
        );
    }

    #[test]
    fn severity_override_returns_user_choice() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [rules.max-function-lines]
            severity = "error"
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert!(matches!(
            cfg.severity_for("max-function-lines"),
            Some(Severity::Error)
        ));
        assert!(cfg.severity_for("max-file-lines").is_none());
    }

    #[test]
    fn missing_config_falls_back_to_defaults() {
        let tmp = TempDir::new().unwrap();
        // No comply.toml in tmp or any ancestor (we may walk up to a
        // real one, that's fine — what we test is "no panic").
        let cfg = Config::load_from(tmp.path()).unwrap();
        // The default for max-function-lines is 30, regardless of
        // whether we walked into a real workspace.
        let _ = cfg.threshold("max-function-lines", "max", Language::TypeScript);
    }

    #[test]
    fn print_default_toml_renders_expected_section() {
        let text = Config::print_default_toml();
        // toml::to_string_pretty inlines empty parents and emits the
        // dotted form, so we expect `[rules.max-function-lines]`.
        assert!(text.contains("[rules.max-function-lines]"));
        assert!(text.contains("max ="));
    }

    #[test]
    fn lang_qualified_key_overrides_base() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("comply.toml"),
            r#"
            [rules."max-function-lines.ts"]
            max = 50
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::TypeScript), 50);
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::Rust), 30);
    }

    #[test]
    fn lang_brace_expansion_matches_multiple() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("comply.toml"),
            r#"
            [rules."max-function-lines.{ts,rs}"]
            max = 60
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::TypeScript), 60);
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::Rust), 60);
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::Sql), 30);
    }

    #[test]
    fn lang_glob_star_matches_prefix() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("comply.toml"),
            r#"
            [rules."max-function-lines.ts*"]
            max = 40
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::TypeScript), 40);
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::Tsx), 40);
        assert_eq!(cfg.threshold("max-function-lines", "max", Language::Rust), 30);
    }

    #[test]
    fn lang_qualified_key_falls_through_for_missing_keys() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("comply.toml"),
            r#"
            [rules."id-length.rs"]
            min = 3
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert_eq!(cfg.threshold("id-length", "min", Language::Rust), 3);
        let exceptions = cfg.string_list("id-length", "exceptions", Language::Rust);
        assert_eq!(exceptions, vec!["_", "t", "T"]);
    }

    #[test]
    fn lang_qualified_key_removed_from_iter_rules() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("comply.toml"),
            r#"
            [rules."max-function-lines.ts"]
            max = 50
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert!(cfg.iter_rules().all(|(id, _)| id == "max-function-lines" || !id.contains(".ts")));
    }
}
