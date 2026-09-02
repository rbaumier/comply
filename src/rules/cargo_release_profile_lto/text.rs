//! Line-oriented scan of a `Cargo.toml`: section headers plus `key = value`
//! lines are all the rule needs, so it doesn't round-trip TOML.
//!
//! Three questions, in order:
//!
//! 1. **Is this the manifest Cargo reads profiles from?** A manifest with
//!    `[workspace]` is; so is a standalone `[package]` with neither a
//!    `workspace` key of its own nor a `Cargo.toml` in an ancestor directory
//!    inside the project. A member crate's `[profile.*]` is ignored by Cargo,
//!    so flagging one would ask for a no-op edit.
//! 2. **Is it exempt?** A `[lib] proc-macro = true` crate builds a compiler
//!    plugin whose own codegen settings don't shape the shipped binary.
//! 3. **Does `[profile.release]` set `lto` and `codegen-units = 1`?** Values
//!    reached through `inherits = "…"` count, so a manifest that defines the
//!    settings once on a base profile and inherits them is fine.

use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// `lto` values that enable link-time optimization. `"thin"` is the
/// build-time-cheaper variant and is accepted; `false` / `"off"` are not.
const ENABLED_LTO_VALUES: &[&str] = &["true", "\"fat\"", "'fat'", "\"thin\"", "'thin'"];

/// The only `codegen-units` value that lets LLVM see the whole crate at once.
const SINGLE_CODEGEN_UNIT: &str = "1";

/// Upper bound on the `inherits = "…"` chain walk. Cargo profile chains are a
/// hop or two deep; the bound only exists so a manifest with a cyclic
/// `inherits` cannot spin.
const MAX_INHERITS_HOPS: usize = 8;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.path.file_name().is_none_or(|name| name != "Cargo.toml") {
            return Vec::new();
        }
        let manifest = Manifest::parse(ctx.source);
        if !manifest.is_profile_root(ctx) || manifest.builds_proc_macro {
            return Vec::new();
        }

        let release = manifest.profile("release");
        let mut missing: Vec<&str> = Vec::new();
        if !manifest
            .resolved(release, profile_lto)
            .is_some_and(|value| ENABLED_LTO_VALUES.contains(&value))
        {
            missing.push("`lto = \"fat\"`");
        }
        if manifest.resolved(release, profile_codegen_units) != Some(SINGLE_CODEGEN_UNIT) {
            missing.push("`codegen-units = 1`");
        }
        if missing.is_empty() {
            return Vec::new();
        }

        let missing = missing.join(" and ");
        let message = match release {
            Some(_) => format!(
                "`[profile.release]` is missing {missing} — release builds compile each crate \
                 separately, without cross-crate inlining. `lto = \"thin\"` is the cheaper \
                 compromise when release build time matters."
            ),
            None => format!(
                "root `Cargo.toml` declares no `[profile.release]` — release builds ship without \
                 link-time optimization. Add the section with {missing}."
            ),
        };
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            // A missing section has no line of its own, so it anchors on line 1.
            line: release.map_or(1, |profile| profile.header_line),
            column: 1,
            rule_id: super::META.id.into(),
            message,
            severity: Severity::Error,
            span: None,
        }]
    }
}

/// One `[profile.<name>]` section, with only the keys this rule reads.
#[derive(Debug)]
struct Profile {
    name: String,
    header_line: usize,
    lto: Option<String>,
    codegen_units: Option<String>,
    inherits: Option<String>,
}

fn profile_lto(profile: &Profile) -> Option<&str> {
    profile.lto.as_deref()
}

fn profile_codegen_units(profile: &Profile) -> Option<&str> {
    profile.codegen_units.as_deref()
}

#[derive(Debug, Default)]
struct Manifest {
    has_workspace: bool,
    has_package: bool,
    /// `[package] workspace = …` — the crate explicitly points at a workspace
    /// root, so it is a member whatever its directory layout looks like.
    package_declares_workspace: bool,
    builds_proc_macro: bool,
    profiles: Vec<Profile>,
}

impl Manifest {
    fn parse(source: &str) -> Self {
        let mut manifest = Manifest::default();
        let mut section = String::new();
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                section = section_name(trimmed);
                match section.as_str() {
                    "workspace" => manifest.has_workspace = true,
                    "package" => manifest.has_package = true,
                    _ => {}
                }
                if let Some(name) = section.strip_prefix("profile.")
                    // `[profile.release.package.foo]` overrides one dependency,
                    // not the profile itself.
                    && !name.contains('.')
                {
                    manifest.profiles.push(Profile {
                        name: name.to_owned(),
                        header_line: index + 1,
                        lto: None,
                        codegen_units: None,
                        inherits: None,
                    });
                }
                continue;
            }
            let Some((key, value)) = key_value(trimmed) else {
                continue;
            };
            match (section.as_str(), key) {
                ("package", "workspace") => manifest.package_declares_workspace = true,
                ("lib", "proc-macro") => manifest.builds_proc_macro = value == "true",
                _ => {
                    if section.starts_with("profile.")
                        && let Some(profile) = manifest.profiles.last_mut()
                    {
                        match key {
                            "lto" => profile.lto = Some(value.to_owned()),
                            "codegen-units" => profile.codegen_units = Some(value.to_owned()),
                            "inherits" => profile.inherits = Some(unquote(value).to_owned()),
                            _ => {}
                        }
                    }
                }
            }
        }
        manifest
    }

    fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    /// The value of one key on `start`, following `inherits = "…"` when the
    /// profile doesn't set it itself.
    fn resolved<'a>(
        &'a self,
        start: Option<&'a Profile>,
        key: fn(&Profile) -> Option<&str>,
    ) -> Option<&'a str> {
        let mut current = start?;
        for _ in 0..MAX_INHERITS_HOPS {
            if let Some(value) = key(current) {
                return Some(value);
            }
            current = self.profile(current.inherits.as_deref()?)?;
        }
        None
    }

    fn is_profile_root(&self, ctx: &CheckCtx) -> bool {
        if self.has_workspace {
            return true;
        }
        if !self.has_package || self.package_declares_workspace {
            return false;
        }
        !has_ancestor_manifest(ctx)
    }
}

/// The section name of a header line, without brackets — `[profile.release]`
/// becomes `profile.release`. An unterminated header yields the rest of the
/// line, which matches no section this rule cares about.
fn section_name(header: &str) -> String {
    let body = header.trim_start_matches('[');
    body.split(']').next().unwrap_or(body).trim().to_owned()
}

/// Split a `key = value` line, dropping a trailing `# comment`. Returns `None`
/// for blank lines, comments, and anything without an `=`.
fn key_value(line: &str) -> Option<(&str, &str)> {
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let value = value.split('#').next().unwrap_or(value);
    Some((key.trim(), value.trim()))
}

fn unquote(value: &str) -> &str {
    value.trim_matches(['"', '\''])
}

/// True when a `Cargo.toml` sits in an ancestor directory of this manifest,
/// still inside the project — the mark of a workspace member listed by a root
/// above it. Without a known project root the walk is skipped rather than left
/// to escape into the user's home directory.
fn has_ancestor_manifest(ctx: &CheckCtx) -> bool {
    let Some(root) = ctx.project.project_root.as_deref() else {
        return false;
    };
    let Some(manifest_dir) = ctx.path.parent() else {
        return false;
    };
    let mut candidate: Option<&Path> = manifest_dir.parent();
    while let Some(directory) = candidate {
        if !directory.starts_with(root) {
            return false;
        }
        if directory.join("Cargo.toml").is_file() {
            return true;
        }
        candidate = directory.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Cargo.toml"), source))
    }

    #[test]
    fn flags_workspace_root_without_release_profile() {
        let src = "[workspace]\nmembers = [\"a\"]\n";
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
        assert!(diagnostics[0].message.contains("no `[profile.release]`"));
    }

    #[test]
    fn flags_release_profile_without_lto_or_codegen_units() {
        let src = "[workspace]\n\n[profile.release]\nstrip = true\n";
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        // Anchored on the section header, not on line 1.
        assert_eq!(diagnostics[0].line, 3);
        assert!(diagnostics[0].message.contains("`lto = \"fat\"` and `codegen-units = 1`"));
    }

    #[test]
    fn flags_release_profile_missing_only_codegen_units() {
        let src = "[workspace]\n\n[profile.release]\nlto = \"fat\"\n";
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("missing `codegen-units = 1`"));
    }

    #[test]
    fn flags_release_profile_with_lto_disabled() {
        let src = "[workspace]\n\n[profile.release]\nlto = false\ncodegen-units = 1\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_standalone_package_without_release_profile() {
        let src = "[package]\nname = \"tool\"\nversion = \"0.1.0\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_release_profile_with_fat_lto_and_one_codegen_unit() {
        let src = "[workspace]\n\n[profile.release]\nlto = \"fat\"\ncodegen-units = 1\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_thin_lto() {
        let src = "[workspace]\n\n[profile.release]\nlto = \"thin\"\ncodegen-units = 1\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_boolean_lto() {
        let src = "[package]\nname = \"tool\"\n\n[profile.release]\nlto = true\ncodegen-units = 1 # whole-crate\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_settings_reached_through_inherits() {
        let src = "[workspace]\n\n[profile.optimized]\nlto = \"fat\"\ncodegen-units = 1\n\n\
                   [profile.release]\ninherits = \"optimized\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_member_crate_pointing_at_a_workspace() {
        let src = "[package]\nname = \"member\"\nworkspace = \"..\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_proc_macro_crate() {
        let src = "[package]\nname = \"derive-like\"\n\n[lib]\nproc-macro = true\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_cargo_toml_file() {
        let diagnostics =
            Check.check(&CheckCtx::for_test(Path::new("rustfmt.toml"), "[workspace]\n"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn allows_member_crate_under_a_workspace_root_on_disk() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/member\"]\n\n[profile.release]\nlto = \"fat\"\ncodegen-units = 1\n",
        )
        .expect("write root manifest");
        let member_dir = dir.path().join("crates/member");
        std::fs::create_dir_all(&member_dir).expect("create member dir");
        let member_manifest = member_dir.join("Cargo.toml");
        let source = "[package]\nname = \"member\"\nversion = \"0.1.0\"\n";
        std::fs::write(&member_manifest, source).expect("write member manifest");

        let mut project = crate::project::ProjectCtx::default();
        project.project_root = Some(dir.path().to_path_buf());
        let ctx = CheckCtx::for_test_with_project(&member_manifest, source, &project);
        assert!(Check.check(&ctx).is_empty());
    }
}
