//! Shared helpers for zod rules that gate a Zod-v4-only suggestion on the
//! project's resolved zod version.
//!
//! Top-level format helpers (`z.email()`, `z.url()`, `z.int()`, …) exist only
//! in zod v4. A rule that suggests one must fire only when the nearest
//! `package.json` proves zod resolves to v4 or later; on zod v3 (or an
//! unresolvable version) the suggested API does not exist, so applying it would
//! be a runtime `TypeError`.

use crate::project::PackageJson;
use crate::rules::backend::CheckCtx;

/// True when the nearest `package.json` proves zod resolves to v4 or later.
///
/// Looks across every dependency section, and — because the zod package itself
/// does not list `zod` as a dependency — falls back to the manifest's own
/// top-level `version` when it is the zod package (`name == "zod"`). When the
/// version cannot be proven >= 4 (no manifest, undeclared, or a range whose
/// smallest major is < 4 such as `^3 || ^4`), this returns `false`.
pub fn zod_is_v4_or_later(ctx: &CheckCtx) -> bool {
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return false;
    };
    zod_version_range(&pkg)
        .and_then(range_min_major)
        .is_some_and(|major| major >= 4)
}

/// The declared zod version range from the nearest manifest: a dependency entry
/// in any section, or the manifest's own `version` when it is the zod package.
fn zod_version_range(pkg: &PackageJson) -> Option<&str> {
    pkg.dependencies
        .get("zod")
        .or_else(|| pkg.dev_dependencies.get("zod"))
        .or_else(|| pkg.peer_dependencies.get("zod"))
        .or_else(|| pkg.optional_dependencies.get("zod"))
        .map(String::as_str)
        .or_else(|| (pkg.name.as_deref() == Some("zod")).then(|| pkg.version.as_deref())?)
}

/// Smallest major version a range can resolve to. Splits on `||`, takes the
/// first numeric run of each alternative as its major, and returns the minimum
/// across alternatives. Returns `None` when no alternative contains a number,
/// so undeterminable ranges (e.g. `latest`, `*`, a workspace/git spec) do not
/// fire. `^3 || ^4` yields `Some(3)`, keeping v3-compatible projects silent.
fn range_min_major(range: &str) -> Option<u32> {
    range.split("||").filter_map(first_numeric_run).min()
}

/// First contiguous run of ASCII digits in `s`, parsed as a `u32`. Skips any
/// leading non-digit prefix (`^`, `~`, `>=`, `v`, whitespace).
fn first_numeric_run(s: &str) -> Option<u32> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let end = s[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map_or(s.len(), |offset| start + offset);
    s[start..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_min_major_takes_smallest_alternative() {
        assert_eq!(range_min_major("^3 || ^4"), Some(3));
        assert_eq!(range_min_major("^4 || ^3"), Some(3));
        assert_eq!(range_min_major(">=4.0.0"), Some(4));
        assert_eq!(range_min_major("4.1.0"), Some(4));
        assert_eq!(range_min_major("latest"), None);
        assert_eq!(range_min_major("*"), None);
    }
}
