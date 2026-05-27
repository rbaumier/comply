//! Declarative framework conventions, embedded as TOML.
//!
//! Adding a new framework = one `.toml` file + one `include_str!` line.
//! Rules ask "what counts as an entry point?" and get the union of every
//! matching framework's declarations.

use crate::project::PackageJson;
use serde::Deserialize;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct FrameworkDef {
    #[serde(skip)]
    pub name: String,
    pub detection: Detection,
    #[serde(default)]
    pub entry_points: EntryPoints,
    #[serde(default)]
    pub magic_exports: MagicExports,
    #[serde(default)]
    pub tooling_deps: ToolingDeps,
}

#[derive(Debug, Default, Deserialize)]
pub struct Detection {
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct EntryPoints {
    #[serde(default)]
    pub dirs: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub root_files: Vec<String>,
    #[serde(default)]
    pub file_suffixes: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MagicExports {
    #[serde(default)]
    pub names: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ToolingDeps {
    #[serde(default)]
    pub names: Vec<String>,
}

const RAW: &[(&str, &str)] = &[
    ("nextjs", include_str!("nextjs.toml")),
    ("remix", include_str!("remix.toml")),
    ("vite", include_str!("vite.toml")),
    ("express", include_str!("express.toml")),
    ("jest", include_str!("jest.toml")),
    ("playwright", include_str!("playwright.toml")),
    ("elysia", include_str!("elysia.toml")),
    ("tanstack-router", include_str!("tanstack-router.toml")),
    ("shadcn", include_str!("shadcn.toml")),
    ("react-email", include_str!("react-email.toml")),
    ("react-native", include_str!("react-native.toml")),
    ("webpack", include_str!("webpack.toml")),
    ("mocha", include_str!("mocha.toml")),
    ("drizzle", include_str!("drizzle.toml")),
    ("better-result", include_str!("better-result.toml")),
    ("better-auth", include_str!("better-auth.toml")),
    ("tanstack-query", include_str!("tanstack-query.toml")),
    ("angular", include_str!("angular.toml")),
    ("hono", include_str!("hono.toml")),
    ("xstate", include_str!("xstate.toml")),
    ("zod", include_str!("zod.toml")),
    ("i18n", include_str!("i18n.toml")),
];

fn registry() -> &'static [FrameworkDef] {
    static CELL: OnceLock<Vec<FrameworkDef>> = OnceLock::new();
    CELL.get_or_init(|| {
        RAW.iter()
            .map(|(stem, body)| {
                let mut def: FrameworkDef =
                    toml::from_str(body).unwrap_or_else(|e| panic!("{stem}.toml malformed: {e}"));
                def.name = (*stem).to_string();
                def
            })
            .collect()
    })
}

#[cfg(test)]
pub fn get_framework(name: &str) -> Option<&'static FrameworkDef> {
    registry().iter().find(|def| def.name == name)
}

/// Every registered framework — lets tests build a `ProjectCtx` that
/// unlocks all framework-scoped rules without enumerating names by hand.
#[cfg(test)]
pub fn all() -> Vec<&'static FrameworkDef> {
    registry().iter().collect()
}

pub fn detect_frameworks(
    pkg: &PackageJson,
    project_root: Option<&Path>,
) -> Vec<&'static FrameworkDef> {
    registry()
        .iter()
        .filter(|def| {
            let by_dep = def
                .detection
                .dependencies
                .iter()
                .any(|d| pkg.has_dep_or_engine(d));
            if by_dep {
                return true;
            }
            if let Some(root) = project_root {
                return def
                    .detection
                    .files
                    .iter()
                    .any(|file| root.join(file).metadata().is_ok());
            }
            false
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tomls_parse_cleanly() {
        let r = registry();
        assert!(
            r.len() >= 6,
            "expected at least 6 frameworks, got {}",
            r.len()
        );
        for def in r {
            assert!(!def.name.is_empty());
            assert!(
                !def.detection.dependencies.is_empty() || !def.detection.files.is_empty(),
                "framework {} must declare at least one dependency or detection file",
                def.name
            );
        }
    }

    #[test]
    fn detects_nextjs() {
        let mut pkg = PackageJson::default();
        pkg.dependencies.insert("next".into(), "14.0.0".into());
        let matched = detect_frameworks(&pkg, None);
        assert!(matched.iter().any(|f| f.name == "nextjs"));
    }

    #[test]
    fn detects_jest_in_dev_deps() {
        let mut pkg = PackageJson::default();
        pkg.dev_dependencies.insert("jest".into(), "29.0.0".into());
        let matched = detect_frameworks(&pkg, None);
        assert!(matched.iter().any(|f| f.name == "jest"));
    }

    #[test]
    fn no_match_with_empty_pkg() {
        let pkg = PackageJson::default();
        assert!(detect_frameworks(&pkg, None).is_empty());
    }

    #[test]
    fn multiple_frameworks_match() {
        let mut pkg = PackageJson::default();
        pkg.dependencies.insert("next".into(), "14.0.0".into());
        pkg.dev_dependencies.insert("jest".into(), "29.0.0".into());
        pkg.dev_dependencies
            .insert("@playwright/test".into(), "1.40.0".into());
        let matched = detect_frameworks(&pkg, None);
        assert!(matched.len() >= 3);
    }
}
