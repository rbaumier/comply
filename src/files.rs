//! File discovery — finds lintable files via directory walk or git diff.
//!
//! - `ScanMode::All` → directory walk via `ignore` crate (standard_filters
//!   excludes .git/, node_modules/, target/). Also honors `.gitignore`,
//!   `.ignore`, and a comply-specific `.complyignore` (same gitignore
//!   syntax — useful to skip files in comply without affecting git). Hidden
//!   dot-directories are skipped, except a small allowlist of well-known
//!   config dirs (`.storybook/`, `.vscode/`) whose real ES imports must
//!   register in the cross-file import index.
//! - Git modes → shell out to `git diff` / `git show` and validate exit
//!   status (silent empty output used to mask real failures).
//! - Each file is classified by extension into a Language; unknown
//!   extensions are silently skipped.

use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::ScanMode;

const TS_EXTENSIONS: &[&str] = &["ts", "mts"];
const TSX_EXTENSIONS: &[&str] = &["tsx", "jsx"];
const JS_EXTENSIONS: &[&str] = &["js", "mjs"];
const RUST_EXTENSIONS: &[&str] = &["rs"];
const VUE_EXTENSIONS: &[&str] = &["vue"];
const SVELTE_EXTENSIONS: &[&str] = &["svelte"];
const TOML_EXTENSIONS: &[&str] = &["toml"];
const JSON_EXTENSIONS: &[&str] = &["json"];
const CSS_EXTENSIONS: &[&str] = &["css"];
const YAML_EXTENSIONS: &[&str] = &["yml", "yaml"];
const DOCKERFILE_EXTENSIONS: &[&str] = &["dockerfile"];
const SQL_EXTENSIONS: &[&str] = &["sql"];
const GRAPHQL_EXTENSIONS: &[&str] = &["graphql", "gql"];
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "mdx"];
const ASTRO_EXTENSIONS: &[&str] = &["astro"];
const HTML_EXTENSIONS: &[&str] = &["html", "htm"];

/// A discovered file tagged with its detected language.
#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

/// The detected source language. TS and Tsx are kept distinct so the engine
/// can pick the correct tree-sitter grammar — TSX requires `LANGUAGE_TSX`,
/// otherwise JSX syntax produces ERROR nodes and bogus diagnostics.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// Plain `.ts` / `.mts`.
    TypeScript,
    /// `.tsx` / `.jsx` — needs the JSX-aware grammar.
    Tsx,
    /// Plain JavaScript `.js` / `.mjs` — handled by the TypeScript grammar
    /// since it's a strict superset.
    JavaScript,
    /// Rust source `.rs`.
    Rust,
    /// Vue Single-File Component `.vue` — text-based rules only (no
    /// tree-sitter grammar bundled). Rules check the raw SFC source
    /// for template/script patterns.
    Vue,
    /// TOML configuration file `.toml` — text-based rules only, parsed
    /// on demand by individual rules via the `toml` crate.
    Toml,
    /// JSON data file `.json` — text-based rules only, parsed on demand
    /// by individual rules via the `serde_json` crate. Used for i18n
    /// translation files, config files, etc.
    Json,
    /// CSS stylesheet `.css` — text-based rules only.
    Css,
    /// YAML file `.yml` / `.yaml` — text-based rules only. Used for
    /// Kubernetes manifests, docker-compose, GitHub Actions workflows.
    Yaml,
    /// Dockerfile — text-based rules only. Matched by extension
    /// `.dockerfile` or by filename starting with `Dockerfile`.
    Dockerfile,
    /// SQL file `.sql` — text-based rules only.
    Sql,
    /// GraphQL schema or operation file `.graphql` / `.gql` — text-based
    /// rules only.
    GraphQl,
    /// Svelte component `.svelte` — text-based rules only.
    Svelte,
    /// Markdown / MDX documentation `.md` / `.mdx`. Indexed only for the ESM
    /// `import` statements at the top of the file (MDX/Docusaurus/Nextra/Astro
    /// process these at build time), so a component consumed exclusively from a
    /// docs page is recorded as a real cross-file usage. No tree-sitter grammar
    /// is bundled; no lint rule targets it.
    Markdown,
    /// Astro component `.astro`. Indexed only for the ESM `import` statements in
    /// the frontmatter script block (between the leading `---` fences), so a
    /// module consumed exclusively from an `.astro` file is recorded as a real
    /// cross-file usage. No tree-sitter grammar is bundled; no lint rule targets
    /// it.
    Astro,
    /// HTML document `.html` / `.htm`. Indexed only for the local
    /// `<script src="…">` bundler entries it declares (Parcel/Vite/plain-ESM load
    /// the app from an HTML file), so the entry module a `<script>` points at —
    /// and everything it transitively imports — is recorded as reachable. No
    /// tree-sitter grammar is bundled; no lint rule targets it.
    Html,
}

impl Language {
    /// Short suffix used as the language qualifier in per-language config
    /// keys, e.g. `[rules."id-length.ts"]`. Matches the canonical file
    /// extension for the language.
    pub fn config_suffix(self) -> &'static str {
        match self {
            Language::TypeScript => "ts",
            Language::Tsx => "tsx",
            Language::JavaScript => "js",
            Language::Rust => "rs",
            Language::Vue => "vue",
            Language::Toml => "toml",
            Language::Json => "json",
            Language::Css => "css",
            Language::Yaml => "yaml",
            Language::Dockerfile => "dockerfile",
            Language::Sql => "sql",
            Language::GraphQl => "graphql",
            Language::Svelte => "svelte",
            Language::Markdown => "md",
            Language::Astro => "astro",
            Language::Html => "html",
        }
    }

    /// True for a language indexed only for the cross-file imports it declares
    /// and dispatched to no lint engine (Markdown / Astro / HTML). A file in such
    /// a language is never visited by a rule, so it must never be chosen as a
    /// once-per-project anchor: a cross-file rule keyed to that anchor would find
    /// no dispatched file matching it and silently emit nothing.
    pub fn is_import_index_only(self) -> bool {
        matches!(self, Language::Markdown | Language::Astro | Language::Html)
    }

    /// True if the language is a TypeScript/JavaScript variant — used by the
    /// orchestrator to dispatch to oxlint.
    pub fn is_typescript_family(self) -> bool {
        matches!(
            self,
            Language::TypeScript | Language::Tsx | Language::JavaScript
        )
    }

    /// Detect the language from a file path's extension. Returns `None`
    /// for extensions comply doesn't recognize. Used by the LSP server,
    /// which receives URIs from the editor and needs to decide whether
    /// the buffer is in scope before running the lint pass.
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        if name.ends_with(".d.ts")
            || name.ends_with(".d.mts")
            || name.ends_with(".d.cts")
            || name.ends_with(".d.tsx")
        {
            return None;
        }
        // Files with no extension (e.g. `Dockerfile`, `Dockerfile.prod`) must be
        // checked by name BEFORE the extension-based early-return below.
        if name == "Dockerfile" || name.starts_with("Dockerfile.") {
            return Some(Language::Dockerfile);
        }
        let ext = path.extension()?.to_str()?;
        if TS_EXTENSIONS.contains(&ext) {
            Some(Language::TypeScript)
        } else if TSX_EXTENSIONS.contains(&ext) {
            Some(Language::Tsx)
        } else if JS_EXTENSIONS.contains(&ext) {
            Some(Language::JavaScript)
        } else if RUST_EXTENSIONS.contains(&ext) {
            Some(Language::Rust)
        } else if VUE_EXTENSIONS.contains(&ext) {
            Some(Language::Vue)
        } else if SVELTE_EXTENSIONS.contains(&ext) {
            Some(Language::Svelte)
        } else if TOML_EXTENSIONS.contains(&ext) {
            Some(Language::Toml)
        } else if JSON_EXTENSIONS.contains(&ext) {
            Some(Language::Json)
        } else if CSS_EXTENSIONS.contains(&ext) {
            Some(Language::Css)
        } else if YAML_EXTENSIONS.contains(&ext) {
            Some(Language::Yaml)
        } else if SQL_EXTENSIONS.contains(&ext) {
            Some(Language::Sql)
        } else if GRAPHQL_EXTENSIONS.contains(&ext) {
            Some(Language::GraphQl)
        } else if MARKDOWN_EXTENSIONS.contains(&ext) {
            Some(Language::Markdown)
        } else if ASTRO_EXTENSIONS.contains(&ext) {
            Some(Language::Astro)
        } else if HTML_EXTENSIONS.contains(&ext) {
            Some(Language::Html)
        } else if DOCKERFILE_EXTENSIONS.contains(&ext)
            || path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "Dockerfile" || n.starts_with("Dockerfile."))
        {
            Some(Language::Dockerfile)
        } else {
            None
        }
    }
}

/// Discover files to lint based on the resolved scan mode.
#[must_use = "discovered files must be linted or the scan was wasted"]
pub fn discover(mode: &ScanMode) -> Result<Vec<SourceFile>> {
    match mode {
        ScanMode::All(path) => walk_directory(path),
        ScanMode::WorkingTree => git_diff_files(&[]),
        ScanMode::Staged => git_diff_files(&["--cached"]),
        // `HEAD~1 HEAD` — without the second `HEAD`, git diffs against the
        // working tree and mixes unstaged changes into "last commit" results.
        ScanMode::LastCommit => git_diff_files(&["HEAD~1", "HEAD"]),
        ScanMode::Commit(sha) => git_show_files(sha),
        ScanMode::Range(from, to) => git_diff_files(&[from.as_str(), to.as_str()]),
    }
}

/// Walk a directory tree and classify every file.
const EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".output",
    "coverage",
    "plans",
    "documents",
    ".git",
    "vendor",
    "vendors",
    "vendored",
    "external",
    "third-party",
    "third_party",
];

/// Hidden (dot-prefixed) directories that hold real TypeScript/JavaScript with
/// genuine ES `import` statements: `.storybook/main.ts` imports framework
/// presets and addons, `.vscode/` holds editor task/extension scripts, and
/// `.vitepress/` is the VitePress documentation-site config dir whose build-time
/// modules (`config.ts`, `vite.config.ts`, plugins) import from project source.
/// The directory walk filters hidden files by default, so these imports never
/// reach the cross-file import index — making the packages and modules they
/// import look unused. Walking these well-known config dirs registers their
/// imports as real usage.
const SCANNED_CONFIG_DOT_DIRS: &[&str] = &[".storybook", ".vitepress", ".vscode"];

/// True if any path segment is a hidden directory that is *not* a known config
/// dir we deliberately scan. The directory walk disables the `ignore` crate's
/// hidden filter so config dot-dirs can be reached, so this restores the
/// "skip hidden" behaviour for every other dot-path.
fn is_hidden_non_config(path: &Path) -> bool {
    path.iter().filter_map(|seg| seg.to_str()).any(|seg| {
        seg.starts_with('.')
            && seg != "."
            && seg != ".."
            && !SCANNED_CONFIG_DOT_DIRS.contains(&seg)
    })
}

/// Build the directory walker comply uses to scan a project: standard ignore
/// files plus the project's own `.complyignore`/ESLint exclusions, with
/// `EXCLUDED_DIRS` and non-config hidden paths pruned. Shared by every
/// directory-scan entry point so they observe identical exclusion semantics.
fn project_walker(path: &Path) -> ignore::Walk {
    // Honor the project's own ESLint config-based exclusions (flat-config
    // `ignores`, `ignorePatterns`, package.json eslintConfig) as a blanket walk
    // prune — these are directory/build-artifact patterns safe to skip for every
    // language. The file-based `.eslintignore` / `.eslint-ignore` are handled
    // separately in `walk_directory`, scoped to the TypeScript family, because
    // they routinely list non-JS globs (`*.rs`) that must not prune other
    // languages.
    let eslint_ignore = crate::project::eslint_ignore::load(path);
    // The root itself may be a hidden/absolute path; only entries *below* it are
    // checked against the hidden filter, so strip the root prefix first.
    let root = path.to_path_buf();
    WalkBuilder::new(path)
        .standard_filters(true)
        // Disable the crate's blanket hidden filter so config dot-dirs are
        // reachable; `filter_entry` below re-prunes every other hidden path.
        .hidden(false)
        .add_custom_ignore_filename(".complyignore")
        .filter_entry(move |entry| {
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
            if is_dir
                && let Some(name) = entry.file_name().to_str()
                && EXCLUDED_DIRS.contains(&name)
            {
                return false;
            }
            // Re-prune hidden paths the crate's `hidden` filter would have
            // dropped, except the config dot-dirs we deliberately scan. Only
            // segments below the scan root count, so a hidden root stays in.
            if let Ok(rel) = entry.path().strip_prefix(&root)
                && is_hidden_non_config(rel)
            {
                return false;
            }
            // Prune directories matching a config-derived ignore so their
            // subtree is never walked; also drops individual matching files.
            if let Some(gi) = &eslint_ignore
                && gi.matched(entry.path(), is_dir).is_ignore()
            {
                return false;
            }
            true
        })
        .build()
}

fn walk_directory(path: &Path) -> Result<Vec<SourceFile>> {
    // ESLint's file-based ignores (`.eslintignore` / `.eslint-ignore`, commonly
    // a `.prettierignore` symlink) list non-JS globs like `*.rs` that ESLint
    // never lints, so they only exclude files in the TypeScript family — never
    // Rust or any other language. Built once, matched per file below.
    let eslint_ignore_files = crate::project::eslint_ignore::load_ignore_files(path);
    let mut files = Vec::new();
    for entry in project_walker(path) {
        let entry = entry.context("failed to read directory entry")?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        if let Some(sf) = classify(entry.path()) {
            if sf.language.is_typescript_family()
                && let Some(gi) = &eslint_ignore_files
                && gi.matched_path_or_any_parents(&sf.path, false).is_ignore()
            {
                continue;
            }
            files.push(sf);
        }
    }
    Ok(files)
}

/// TypeScript declaration files (`.d.ts`, `.d.mts`, `.d.cts`, `.d.tsx`) under
/// `path`, using the same walk exclusions as the lint scan (`project_walker`).
/// The TypeScript-family `.eslintignore` filter lives in `walk_directory` and
/// does not apply here — declaration files are never in the lint set anyway,
/// and keeping their references maximizes cross-file usage coverage. Declaration
/// files
/// are dropped from the linted source set, but they carry real
/// `import`/`export … from`/`declare module` references; cross-file rules that
/// reason about dependency usage scan them through this function. Best-effort:
/// unreadable directory entries are skipped rather than aborting the walk.
#[must_use]
pub fn discover_declaration_files(path: &Path) -> Vec<PathBuf> {
    project_walker(path)
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return None;
            }
            let name = entry.path().file_name()?.to_str()?;
            (name.ends_with(".d.ts")
                || name.ends_with(".d.mts")
                || name.ends_with(".d.cts")
                || name.ends_with(".d.tsx"))
            .then(|| entry.path().to_path_buf())
        })
        .collect()
}

/// `git diff --name-only` with the given args. Used for working-tree, staged,
/// last-commit, and range modes.
fn git_diff_files(args: &[&str]) -> Result<Vec<SourceFile>> {
    let mut cmd = Command::new("git");
    cmd.arg("diff")
        .args(args)
        .args(["--name-only", "--diff-filter=d", "--relative"]);
    capture_git_output(cmd, "git diff")
}

/// `git show --name-only` for a single commit — handles initial and merge
/// commits, which `git diff <sha>~1 <sha>` cannot.
fn git_show_files(sha: &str) -> Result<Vec<SourceFile>> {
    let mut cmd = Command::new("git");
    cmd.args(["show", "--name-only", "--pretty=format:", "--diff-filter=d"])
        .arg(sha);
    capture_git_output(cmd, "git show")
}

/// Spawn git, validate exit status, then classify the output paths.
/// Centralizes the bail-on-error pattern so future git modes can't forget it.
fn capture_git_output(mut cmd: Command, label: &str) -> Result<Vec<SourceFile>> {
    let output = cmd
        .output()
        .context("failed to invoke git — is git installed and on PATH?")?;
    if !output.status.success() {
        bail!(
            "{label} failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_git_output(&output.stdout)
}

/// Parse git output line-by-line. Strict UTF-8 — non-UTF-8 paths bail loudly
/// rather than being silently corrupted by `from_utf8_lossy`.
fn parse_git_output(stdout: &[u8]) -> Result<Vec<SourceFile>> {
    let text = std::str::from_utf8(stdout)
        .context("git output contained non-UTF-8 bytes — paths cannot be safely processed")?;
    Ok(text
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| classify(Path::new(l)))
        .collect())
}

/// Classify a file path into a Language based on its extension.
/// Returns None for unsupported extensions (silently skipped).
fn classify(path: &Path) -> Option<SourceFile> {
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && (name.ends_with(".d.ts")
            || name.ends_with(".d.mts")
            || name.ends_with(".d.cts")
            || name.ends_with(".d.tsx"))
        {
            return None;
        }
    let language = if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if TS_EXTENSIONS.contains(&ext) {
            Language::TypeScript
        } else if TSX_EXTENSIONS.contains(&ext) {
            Language::Tsx
        } else if JS_EXTENSIONS.contains(&ext) {
            Language::JavaScript
        } else if RUST_EXTENSIONS.contains(&ext) {
            Language::Rust
        } else if VUE_EXTENSIONS.contains(&ext) {
            Language::Vue
        } else if TOML_EXTENSIONS.contains(&ext) {
            Language::Toml
        } else if JSON_EXTENSIONS.contains(&ext) {
            Language::Json
        } else if CSS_EXTENSIONS.contains(&ext) {
            Language::Css
        } else if YAML_EXTENSIONS.contains(&ext) {
            Language::Yaml
        } else if SQL_EXTENSIONS.contains(&ext) {
            Language::Sql
        } else if GRAPHQL_EXTENSIONS.contains(&ext) {
            Language::GraphQl
        } else if MARKDOWN_EXTENSIONS.contains(&ext) {
            Language::Markdown
        } else if ASTRO_EXTENSIONS.contains(&ext) {
            Language::Astro
        } else if HTML_EXTENSIONS.contains(&ext) {
            Language::Html
        } else if DOCKERFILE_EXTENSIONS.contains(&ext) {
            Language::Dockerfile
        } else {
            return None;
        }
    } else if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "Dockerfile" || n.starts_with("Dockerfile."))
    {
        Language::Dockerfile
    } else {
        return None;
    };
    Some(SourceFile {
        path: path.to_path_buf(),
        language,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang_for(ext: &str) -> Language {
        classify(&PathBuf::from(format!("foo.{ext}")))
            .unwrap()
            .language
    }

    #[test]
    fn classify_routes_extension_to_language() {
        for ext in ["ts", "mts"] {
            assert_eq!(lang_for(ext), Language::TypeScript);
        }
        for ext in ["tsx", "jsx"] {
            assert_eq!(lang_for(ext), Language::Tsx, "{ext} → TSX grammar");
        }
        for ext in ["js", "mjs"] {
            assert_eq!(lang_for(ext), Language::JavaScript);
        }
        assert_eq!(lang_for("rs"), Language::Rust);
    }

    #[test]
    fn classify_skips_unsupported_or_extensionless() {
        for ext in ["txt", "py"] {
            assert!(classify(&PathBuf::from(format!("foo.{ext}"))).is_none());
        }
        assert!(classify(&PathBuf::from("Makefile")).is_none());
    }

    #[test]
    fn classify_markdown_files() {
        // `.md` / `.mdx` are indexed for their top-of-file ESM imports so a
        // component consumed only from a docs page is recorded as used.
        assert_eq!(lang_for("md"), Language::Markdown);
        assert_eq!(lang_for("mdx"), Language::Markdown);
    }

    #[test]
    fn classify_astro_files() {
        // `.astro` is indexed for its frontmatter ESM imports so a module
        // consumed only from an Astro component is recorded as used.
        assert_eq!(lang_for("astro"), Language::Astro);
    }

    #[test]
    fn classify_json_files() {
        assert_eq!(lang_for("json"), Language::Json);
        assert_eq!(lang_for("toml"), Language::Toml);
    }

    #[test]
    fn is_typescript_family_groups_correctly() {
        assert!(Language::TypeScript.is_typescript_family());
        assert!(Language::Tsx.is_typescript_family());
        assert!(Language::JavaScript.is_typescript_family());
        assert!(!Language::Rust.is_typescript_family());
    }

    #[test]
    fn parse_git_output_strict_utf8() {
        assert_eq!(parse_git_output(b"a.ts\nb.rs\n").unwrap().len(), 2);
        // Invalid UTF-8 byte sequence — must error, not corrupt silently.
        assert!(parse_git_output(&[0xFF, 0xFE, b'\n']).is_err());
    }

    fn walked_names(root: &Path) -> Vec<String> {
        walk_directory(root)
            .expect("walk")
            .into_iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn walk_directory_honors_complyignore() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("kept.ts"), "x").unwrap();
        std::fs::write(root.join("skipped.ts"), "x").unwrap();
        std::fs::create_dir(root.join("nested")).unwrap();
        std::fs::write(root.join("nested/also-skipped.ts"), "x").unwrap();
        std::fs::write(root.join(".complyignore"), "skipped.ts\nnested/\n").unwrap();

        let names = walked_names(root);

        assert!(names.contains(&"kept.ts".to_string()));
        assert!(!names.contains(&"skipped.ts".to_string()));
        assert!(!names.contains(&"also-skipped.ts".to_string()));
    }

    #[test]
    fn walk_directory_honors_eslintignore_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("kept.ts"), "x").unwrap();
        std::fs::write(root.join("gen.ts"), "x").unwrap();
        std::fs::write(root.join(".eslintignore"), "gen.ts\n").unwrap();

        let names = walked_names(root);

        assert!(names.contains(&"kept.ts".to_string()));
        assert!(!names.contains(&"gen.ts".to_string()));
    }

    #[test]
    fn walk_directory_eslintignore_scopes_to_typescript_family() {
        // Issue #7657: `.eslintignore` (often a `.prettierignore` symlink) lists
        // non-JS globs like `*.rs` that ESLint/Prettier never process. Those must
        // not suppress Rust discovery, while TS globs — including files inside an
        // ignored directory — are still honored.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("a.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("b.ts"), "x").unwrap();
        std::fs::write(root.join("ignored.ts"), "x").unwrap();
        std::fs::create_dir(root.join("generated")).unwrap();
        std::fs::write(root.join("generated/deep.ts"), "x").unwrap();
        std::fs::write(root.join("generated/keep.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join(".eslintignore"), "*.rs\nignored.ts\ngenerated/\n").unwrap();

        let names = walked_names(root);

        assert!(
            names.contains(&"a.rs".to_string()),
            "a *.rs eslintignore glob must not suppress Rust, got: {names:?}"
        );
        assert!(names.contains(&"b.ts".to_string()));
        assert!(
            !names.contains(&"ignored.ts".to_string()),
            "a TS glob in .eslintignore must still exclude TS, got: {names:?}"
        );
        // A directory pattern must still exclude the TS files under it (the
        // matcher checks the file's ancestors, since the walker no longer prunes
        // the directory), but never the non-TS files.
        assert!(
            !names.contains(&"deep.ts".to_string()),
            "a TS file inside an eslintignore'd directory must still be excluded, got: {names:?}"
        );
        assert!(
            names.contains(&"keep.rs".to_string()),
            "a Rust file inside an eslintignore'd directory must not be suppressed, got: {names:?}"
        );
    }

    #[test]
    fn walk_directory_complyignore_stays_language_agnostic() {
        // `.complyignore` is comply's own ignore file — unlike ESLint's it is not
        // scoped to any language, so a `*.rs` line still suppresses Rust. Guards
        // against accidentally rescoping the wrong ignore file (issue #7657).
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("a.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("b.ts"), "x").unwrap();
        std::fs::write(root.join(".complyignore"), "*.rs\n").unwrap();

        let names = walked_names(root);

        assert!(
            !names.contains(&"a.rs".to_string()),
            ".complyignore must stay language-agnostic, got: {names:?}"
        );
        assert!(names.contains(&"b.ts".to_string()));
    }

    #[test]
    fn walk_directory_honors_flat_config_global_ignores() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("kept.ts"), "x").unwrap();
        std::fs::write(root.join("schema.gen.ts"), "x").unwrap();
        std::fs::write(
            root.join("eslint.config.mjs"),
            "export default [{ ignores: [\"**/*.gen.ts\"] }];\n",
        )
        .unwrap();

        let names = walked_names(root);

        assert!(names.contains(&"kept.ts".to_string()));
        assert!(!names.contains(&"schema.gen.ts".to_string()));
    }

    #[test]
    fn walk_directory_honors_eslintrc_ignore_patterns() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("kept.ts"), "x").unwrap();
        std::fs::create_dir(root.join("build")).unwrap();
        std::fs::write(root.join("build/out.ts"), "x").unwrap();
        std::fs::write(
            root.join(".eslintrc.json"),
            "{ \"ignorePatterns\": [\"build/\"] }",
        )
        .unwrap();

        let names = walked_names(root);

        assert!(names.contains(&"kept.ts".to_string()));
        assert!(!names.contains(&"out.ts".to_string()));
    }

    #[test]
    fn walk_directory_scans_known_config_dot_dirs() {
        // Issue #1769: `.storybook/` and `.vscode/` hold real ES imports; their
        // files must be discovered so those imports register as dependency use.
        // Issue #7058: `.vitepress/` is the VitePress docs-site config dir; its
        // build-time modules import from project source, so a `.vitepress/`
        // nested anywhere (e.g. `packages/.vitepress/`) must also be walked.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir(root.join(".storybook")).unwrap();
        std::fs::write(
            root.join(".storybook/manager.ts"),
            "import { addons } from '@storybook/manager-api';\naddons.setConfig({});",
        )
        .unwrap();
        std::fs::create_dir(root.join(".vscode")).unwrap();
        std::fs::write(root.join(".vscode/tasks.ts"), "export const x = 1;").unwrap();
        std::fs::create_dir_all(root.join("packages/.vitepress/plugins")).unwrap();
        std::fs::write(
            root.join("packages/.vitepress/plugins/markdownTransform.ts"),
            "import { replacer } from '../../../scripts/utils';\nreplacer();",
        )
        .unwrap();

        let names = walked_names(root);

        assert!(
            names.contains(&"manager.ts".to_string()),
            ".storybook files must be walked, got: {names:?}"
        );
        assert!(
            names.contains(&"tasks.ts".to_string()),
            ".vscode files must be walked, got: {names:?}"
        );
        assert!(
            names.contains(&"markdownTransform.ts".to_string()),
            ".vitepress files must be walked, got: {names:?}"
        );
    }

    #[test]
    fn walk_directory_still_skips_other_hidden_dirs() {
        // Only the config allowlist is scanned; stray hidden dirs stay skipped.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("kept.ts"), "x").unwrap();
        std::fs::create_dir(root.join(".cache")).unwrap();
        std::fs::write(root.join(".cache/gen.ts"), "x").unwrap();

        let names = walked_names(root);

        assert!(names.contains(&"kept.ts".to_string()));
        assert!(
            !names.contains(&"gen.ts".to_string()),
            "unlisted hidden dirs must stay skipped, got: {names:?}"
        );
    }

    #[test]
    fn walk_directory_skips_hidden_files_at_root() {
        // A hidden dotfile (`.eslintrc.json`) is not in a config dot-dir and
        // must remain unscanned even with the hidden filter disabled.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("kept.ts"), "x").unwrap();
        std::fs::write(root.join(".hidden.ts"), "x").unwrap();

        let names = walked_names(root);

        assert!(names.contains(&"kept.ts".to_string()));
        assert!(
            !names.contains(&".hidden.ts".to_string()),
            "hidden files outside config dirs must stay skipped, got: {names:?}"
        );
    }

    #[test]
    fn walk_directory_honors_package_json_eslint_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("kept.ts"), "x").unwrap();
        std::fs::write(root.join("vendor.ts"), "x").unwrap();
        std::fs::write(
            root.join("package.json"),
            "{ \"eslintConfig\": { \"ignorePatterns\": [\"vendor.ts\"] } }",
        )
        .unwrap();

        let names = walked_names(root);

        assert!(names.contains(&"kept.ts".to_string()));
        assert!(!names.contains(&"vendor.ts".to_string()));
    }
}
