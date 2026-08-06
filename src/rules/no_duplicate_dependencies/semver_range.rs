//! Whether two npm version ranges can be satisfied by one version.
//!
//! [`are_disjoint`] answers "no single version matches both". It reads the range
//! shapes that carry a decidable answer: `||` alternations of whitespace-joined
//! comparators (`^ ~ >= > <= < =` and bare literals), `x`/`X`/`*` wildcards, and
//! `a - b` hyphen ranges. Versions are plain `major.minor.patch` numbers.
//!
//! The grammar is npm's, which admits `||` and hyphen ranges that Cargo's does
//! not, and the question is whether two ranges intersect rather than whether a
//! version satisfies one. This workspace carries no semver dependency, so the
//! model is built here.
//!
//! Every other shape answers "not disjoint". The caller turns a `true` into a
//! diagnostic, so a shape this module cannot read must never produce one. The
//! shapes it does not read include:
//!
//! * a specifier that is not a semver range at all — `workspace:^1.2.3`,
//!   `catalog:default`, `npm:other@^1`, `file:../sibling`, `link:../sibling`, a
//!   git URL, a dist-tag such as `latest`;
//! * a prerelease or build-metadata version (`1.2.3-beta.1`, `1.2.3+build`),
//!   whose range membership follows extra rules not modelled here;
//! * a version literal npm accepts but this module does not: a `v` prefix
//!   (`v1.2.3`), a fourth numeric component, a number past `u64`;
//! * a wildcard behind an operator (`^*`, `>=x`) or at a hyphen endpoint
//!   (`* - 2.0.0`);
//! * a range whose comparators intersect to nothing (`>=2 <1`).

/// A `major.minor.patch` release. Ordering is semver precedence over the three
/// numeric fields.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    const ZERO: Self = Self {
        major: 0,
        minor: 0,
        patch: 0,
    };
}

/// Which side of its version a bound falls on, so that an inclusive and an
/// exclusive bound on the same version still compare correctly.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Edge {
    /// Immediately below the version — the limit of a `<` comparator.
    JustBelow,
    /// The version itself.
    Exactly,
}

/// A bound on the version line.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Bound {
    version: Version,
    edge: Edge,
}

impl Bound {
    const fn at(version: Version) -> Self {
        Self {
            version,
            edge: Edge::Exactly,
        }
    }

    const fn just_below(version: Version) -> Self {
        Self {
            version,
            edge: Edge::JustBelow,
        }
    }
}

/// A contiguous set of versions: everything from `lowest` through `highest`,
/// both bounds included. `highest` is `None` when the set has no upper limit.
#[derive(Clone, Copy, Debug)]
struct Interval {
    lowest: Bound,
    highest: Option<Bound>,
}

impl Interval {
    /// Every version.
    const ALL: Self = Self {
        lowest: Bound::at(Version::ZERO),
        highest: None,
    };

    /// Every version at or above `start`.
    fn at_least(start: Version) -> Self {
        Self {
            lowest: Bound::at(start),
            highest: None,
        }
    }

    /// Every version at or above `start` and strictly below `end`.
    fn bounded(start: Version, end: Version) -> Self {
        Self {
            lowest: Bound::at(start),
            highest: Some(Bound::just_below(end)),
        }
    }

    /// `Some(self)` when at least one version falls inside, `None` when empty.
    fn non_empty(self) -> Option<Self> {
        self.highest
            .is_none_or(|highest| self.lowest <= highest)
            .then_some(self)
    }

    /// True when at least one version falls in both sets.
    fn overlaps(self, other: Self) -> bool {
        self.intersect(other).is_some()
    }

    /// The versions in both sets, or `None` when they share none.
    fn intersect(self, other: Self) -> Option<Self> {
        let highest = match (self.highest, other.highest) {
            (Some(mine), Some(theirs)) => Some(mine.min(theirs)),
            (only, None) | (None, only) => only,
        };
        Self {
            lowest: self.lowest.max(other.lowest),
            highest,
        }
        .non_empty()
    }
}

/// The operator in front of a comparator. A comparator with no operator carries
/// npm's `=` meaning, so it shares that variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Comparison {
    Caret,
    Tilde,
    Exact,
    AtLeast,
    Above,
    AtMost,
    Below,
}

/// A version literal that names a major, and optionally a minor and a patch:
/// `3`, `3.27`, `3.27.3`, `3.x`.
#[derive(Clone, Copy, Debug)]
struct Prefix {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
}

impl Prefix {
    /// The lowest version the literal covers; absent components read as `0`.
    fn start(self) -> Version {
        Version {
            major: self.major,
            minor: self.minor.unwrap_or(0),
            patch: self.patch.unwrap_or(0),
        }
    }

    /// The first version above every version the literal covers: `1.2.3` ->
    /// `1.2.4`, `1.2` -> `1.3.0`, `1` -> `2.0.0`.
    fn end(self) -> Version {
        match (self.minor, self.patch) {
            (Some(minor), Some(patch)) => Version {
                major: self.major,
                minor,
                patch: patch.saturating_add(1),
            },
            (Some(minor), None) => Version {
                major: self.major,
                minor: minor.saturating_add(1),
                patch: 0,
            },
            (None, _) => Version {
                major: self.major.saturating_add(1),
                minor: 0,
                patch: 0,
            },
        }
    }

    /// The first version above a caret range, which keeps the leftmost non-zero
    /// component: `^1.2.3` -> `2.0.0`, `^0.2.3` -> `0.3.0`, `^0.0.3` -> `0.0.4`,
    /// `^0.0` -> `0.1.0`, `^0` -> `1.0.0`.
    fn caret_end(self) -> Version {
        if self.major > 0 {
            return Version {
                major: self.major.saturating_add(1),
                minor: 0,
                patch: 0,
            };
        }
        match (self.minor, self.patch) {
            (None, _) => Version {
                major: 1,
                minor: 0,
                patch: 0,
            },
            (Some(0), Some(patch)) => Version {
                major: 0,
                minor: 0,
                patch: patch.saturating_add(1),
            },
            (Some(0), None) => Version {
                major: 0,
                minor: 1,
                patch: 0,
            },
            (Some(minor), _) => Version {
                major: 0,
                minor: minor.saturating_add(1),
                patch: 0,
            },
        }
    }

    /// The first version above a tilde range, which allows patch-level changes
    /// once a minor is named: `~1.2.3` and `~1.2` -> `1.3.0`, `~1` -> `2.0.0`.
    fn tilde_end(self) -> Version {
        match self.minor {
            Some(minor) => Version {
                major: self.major,
                minor: minor.saturating_add(1),
                patch: 0,
            },
            None => Version {
                major: self.major.saturating_add(1),
                minor: 0,
                patch: 0,
            },
        }
    }
}

/// A parsed version literal.
#[derive(Clone, Copy, Debug)]
enum Literal {
    /// `*`, `x`, `X`, or the empty string: every version.
    Wildcard,
    Prefix(Prefix),
}

/// True when no single version satisfies both ranges. False whenever either
/// range uses a shape this module does not read — see the module doc.
pub(super) fn are_disjoint(left: &str, right: &str) -> bool {
    let (Some(left), Some(right)) = (parse_range(left), parse_range(right)) else {
        return false;
    };
    !left
        .iter()
        .any(|mine| right.iter().any(|theirs| mine.overlaps(*theirs)))
}

/// Parse a range into the union of version sets it accepts, or `None` when it
/// uses an unread shape.
fn parse_range(text: &str) -> Option<Vec<Interval>> {
    text.split("||").map(parse_alternative).collect()
}

/// Parse one `||` alternative: a hyphen range, or whitespace-separated
/// comparators that all have to hold at once.
fn parse_alternative(text: &str) -> Option<Interval> {
    let text = text.trim();
    if let Some((low, high)) = text.split_once(" - ") {
        return parse_hyphen_range(low, high);
    }
    if text.is_empty() {
        return Some(Interval::ALL);
    }
    let text = attach_operators(text);
    text.split_whitespace()
        .map(parse_comparator)
        .try_fold(Interval::ALL, |accepted, comparator| {
            accepted.intersect(comparator?)
        })
}

/// Drop the whitespace npm allows between a comparator's operator and its
/// version, so that `>= 1.2.3` becomes the single comparator `>=1.2.3`. The
/// whitespace that separates two comparators (`>=1.2.0 <2.0.0`) is kept.
fn attach_operators(text: &str) -> String {
    const OPERATOR_CHARACTERS: [char; 5] = ['<', '>', '=', '^', '~'];
    let mut attached = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_whitespace() && attached.ends_with(OPERATOR_CHARACTERS) {
            continue;
        }
        attached.push(character);
    }
    attached
}

/// Parse `low - high`, which spans from the lowest version `low` names to the
/// highest version `high` names.
fn parse_hyphen_range(low: &str, high: &str) -> Option<Interval> {
    let (Literal::Prefix(low), Literal::Prefix(high)) =
        (parse_literal(low.trim())?, parse_literal(high.trim())?)
    else {
        return None;
    };
    Interval::bounded(low.start(), high.end()).non_empty()
}

fn parse_comparator(text: &str) -> Option<Interval> {
    let (comparison, literal) = split_comparison(text);
    let Literal::Prefix(prefix) = parse_literal(literal)? else {
        // A bare wildcard covers every version; behind an operator it is not a
        // shape this module reads.
        return (comparison == Comparison::Exact).then_some(Interval::ALL);
    };
    let interval = match comparison {
        Comparison::Caret => Interval::bounded(prefix.start(), prefix.caret_end()),
        Comparison::Tilde => Interval::bounded(prefix.start(), prefix.tilde_end()),
        Comparison::Exact => Interval::bounded(prefix.start(), prefix.end()),
        Comparison::AtLeast => Interval::at_least(prefix.start()),
        // `>1.2.3` admits the same versions as `>=1.2.4`, and `>1.2` the same as
        // `>=1.3.0`: there is no release between a prefix and its end.
        Comparison::Above => Interval::at_least(prefix.end()),
        Comparison::AtMost => Interval::bounded(Version::ZERO, prefix.end()),
        Comparison::Below => Interval::bounded(Version::ZERO, prefix.start()),
    };
    interval.non_empty()
}

/// Split a comparator into its operator and the version literal behind it.
fn split_comparison(text: &str) -> (Comparison, &str) {
    // `>=` and `<=` are tried before `>` and `<` so the longer operator wins.
    const OPERATORS: &[(&str, Comparison)] = &[
        (">=", Comparison::AtLeast),
        ("<=", Comparison::AtMost),
        (">", Comparison::Above),
        ("<", Comparison::Below),
        ("=", Comparison::Exact),
        ("^", Comparison::Caret),
        ("~", Comparison::Tilde),
    ];
    for &(operator, comparison) in OPERATORS {
        if let Some(literal) = text.strip_prefix(operator) {
            return (comparison, literal);
        }
    }
    (Comparison::Exact, text)
}

/// Parse a version literal, or `None` when it is not a dotted decimal number
/// with optional trailing `x`/`X`/`*` wildcards.
fn parse_literal(text: &str) -> Option<Literal> {
    if text.is_empty() {
        return Some(Literal::Wildcard);
    }
    let mut numbers: [Option<u64>; 3] = [None; 3];
    for (position, component) in text.split('.').enumerate() {
        let number = numbers.get_mut(position)?;
        if matches!(component, "*" | "x" | "X") {
            // A wildcard component makes every component behind it a wildcard
            // too, so `1.x.3` names the same versions as `1.x`.
            break;
        }
        if component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *number = Some(component.parse().ok()?);
    }
    Some(match numbers[0] {
        None => Literal::Wildcard,
        Some(major) => Literal::Prefix(Prefix {
            major,
            minor: numbers[1],
            patch: numbers[2],
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::are_disjoint;

    #[track_caller]
    fn assert_disjoint(left: &str, right: &str) {
        assert!(are_disjoint(left, right), "{left:?} vs {right:?}");
        assert!(are_disjoint(right, left), "{right:?} vs {left:?} (reversed)");
    }

    #[track_caller]
    fn assert_shares_a_version(left: &str, right: &str) {
        assert!(!are_disjoint(left, right), "{left:?} vs {right:?}");
        assert!(
            !are_disjoint(right, left),
            "{right:?} vs {left:?} (reversed)"
        );
    }

    /// The same assertion as [`assert_shares_a_version`] under a name that says
    /// what is being pinned. `are_disjoint` reports "not disjoint" both for two
    /// ranges that share a version and for a range it cannot read, and the
    /// public predicate cannot tell a caller which of the two it answered.
    #[track_caller]
    fn assert_not_decided(left: &str, right: &str) {
        assert!(!are_disjoint(left, right), "{left:?} vs {right:?}");
        assert!(
            !are_disjoint(right, left),
            "{right:?} vs {left:?} (reversed)"
        );
    }

    #[test]
    fn caret_range_inside_a_wider_caret_range_shares_versions() {
        // The three shapes the published @nuxt/ui manifest declares.
        assert_shares_a_version("^3.27.3", "^3");
        assert_shares_a_version("^3.12.2", "^3.0.0");
        assert_shares_a_version("^4.3.2", "^4.0.0");
    }

    #[test]
    fn caret_ranges_on_different_majors_are_disjoint() {
        assert_disjoint("^3.29.0", "^2");
        assert_disjoint("^1.0.0", "^2.0.0");
    }

    #[test]
    fn caret_keeps_the_leftmost_non_zero_component() {
        assert_disjoint("^0.1.0", "^0.2.0");
        assert_disjoint("^0.0.1", "^0.0.2");
        assert_shares_a_version("^0.1.2", "^0.1.9");
        // `^0` spans every 0.x, `^0.0` only 0.0.x.
        assert_shares_a_version("^0", "^0.5.0");
        assert_disjoint("^0.0", "^0.5.0");
    }

    #[test]
    fn tilde_range_allows_patch_changes_only() {
        assert_shares_a_version("~1.2.3", "^1");
        assert_disjoint("~1.2.3", "~1.3.0");
        assert_shares_a_version("~1", "^1.9.0");
    }

    #[test]
    fn inclusive_and_exclusive_bounds_meet_correctly() {
        assert_disjoint("<1.0.0", ">=1.0.0");
        assert_shares_a_version("<=1.0.0", ">=1.0.0");
        assert_disjoint(">1.0.0", "<=1.0.0");
        assert_shares_a_version(">1.0.0", ">=1.0.1");
    }

    #[test]
    fn partial_versions_behind_a_comparator_widen_to_the_whole_block() {
        // npm reads `>1.2` as `>=1.3.0` and `<=1.2` as `<1.3.0`.
        assert_disjoint(">1.2", "1.2.9");
        assert_shares_a_version("<=1.2", "1.2.9");
        assert_shares_a_version(">=3", "^3.27.3");
    }

    #[test]
    fn wildcards_share_versions_with_everything() {
        assert_shares_a_version("*", "^2.0.0");
        assert_shares_a_version("x", "^2.0.0");
        assert_shares_a_version("", "^2.0.0");
        assert_shares_a_version("1.x", "^1.4.0");
        assert_disjoint("1.x", "^2.0.0");
    }

    #[test]
    fn comparators_on_one_side_of_an_alternative_all_have_to_hold() {
        assert_shares_a_version(">=1.2.0 <2.0.0", "^1.5.0");
        assert_disjoint(">=1.2.0 <2.0.0", "^2.0.0");
    }

    #[test]
    fn alternatives_are_disjoint_only_when_no_branch_overlaps() {
        assert_shares_a_version("^1.0.0 || ^2.0.0", "^2.5.0");
        assert_disjoint("^1.0.0 || ^2.0.0", "^3.0.0");
    }

    #[test]
    fn hyphen_ranges_span_both_endpoints() {
        assert_shares_a_version("1.2.3 - 2.0.0", "^1.9.0");
        assert_shares_a_version("1.2.3 - 2", "^2.9.0");
        assert_disjoint("1.2.3 - 2.0.0", "^3.0.0");
    }

    #[test]
    fn non_semver_specifiers_are_never_disjoint() {
        for specifier in [
            "workspace:^1.2.3",
            "workspace:*",
            "catalog:default",
            "npm:other-package@^1.0.0",
            "file:../sibling",
            "link:../sibling",
            "git+https://github.com/owner/repo.git#v1.0.0",
            "github:owner/repo",
            "latest",
            "next",
        ] {
            assert!(
                !are_disjoint(specifier, "^9.0.0"),
                "{specifier:?} must not be decided"
            );
            assert!(
                !are_disjoint("^9.0.0", specifier),
                "{specifier:?} must not be decided (reversed)"
            );
        }
    }

    #[test]
    fn prerelease_and_build_metadata_are_not_decided() {
        assert_not_decided("1.2.3-beta.1", "^9.0.0");
        assert_not_decided("^1.2.3-beta.1", "^9.0.0");
        assert_not_decided("1.2.3+build.5", "^9.0.0");
    }

    #[test]
    fn an_operator_may_be_separated_from_its_version_by_whitespace() {
        // The published styled-components manifest declares `>= 3.2.0`.
        assert_shares_a_version("3.2.0", ">= 3.2.0");
        assert_disjoint("^2.0.0", ">= 3.2.0");
        assert_shares_a_version("^1.5.0", ">= 1.2.0 < 2.0.0");
        assert_disjoint("^2.0.0", ">= 1.2.0 < 2.0.0");
    }

    #[test]
    fn wildcard_behind_an_operator_is_not_decided() {
        // Each wildcard sits next to a comparator that would make the whole
        // alternative disjoint from `^9.0.0` if the wildcard read as "every
        // version" instead of refusing the alternative.
        assert_not_decided(">=x <1.0.0", "^9.0.0");
        assert_not_decided("^* <1.0.0", "^9.0.0");
    }

    #[test]
    fn a_range_that_accepts_nothing_is_not_decided() {
        assert_not_decided(">=2.0.0 <1.0.0", "^9.0.0");
    }

    #[test]
    fn malformed_literals_are_not_decided() {
        assert_not_decided("1.2.3.4", "^9.0.0");
        assert_not_decided("1..3", "^9.0.0");
        assert_not_decided("^v1.2.3", "^9.0.0");
        assert_not_decided("+1.2.3", "^9.0.0");
        assert_not_decided("99999999999999999999999", "^9.0.0");
        assert_not_decided("1.2.3 - 2.0.0 - 3.0.0", "^9.0.0");
        assert_not_decided("* - 2.0.0", "^9.0.0");
    }

    #[test]
    fn a_component_at_the_top_of_the_number_range_is_not_decided() {
        // The upper bound of `^18446744073709551615` saturates onto its own
        // lower bound, which leaves an empty set rather than a wrong verdict.
        assert_not_decided("^18446744073709551615", "^9.0.0");
    }
}
