//! tailwind-enforces-negative-arbitrary-values backend — flag Tailwind
//! arbitrary values written with a leading minus on the utility prefix
//! (`-top-[1px]`) instead of inside the brackets (`top-[-1px]`). The
//! bracket form is the canonical Tailwind spelling for negative arbitrary
//! values and keeps the sign on the value it actually belongs to.

use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// Matches `-<prop>-[<value>]` where `<value>` does not itself start with `-`.
/// The leading `(^|[^A-Za-z0-9_-])` anchors the minus so we do not match the
/// inner hyphen of identifiers like `foo-top-[1px]`.
fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(^|[^A-Za-z0-9_-])-(?:top|bottom|left|right|m|mt|mb|ml|mr|mx|my|p|pt|pb|pl|pr|px|py|inset|translate|rotate|skew|scale)-\[[^\]\-][^\]]*\]",
        )
        .expect("static regex compiles")
    })
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("className") && !line.contains("class=") && !line.contains("class:")
            {
                continue;
            }
            for m in pattern().find_iter(line) {
                // Column points at the leading minus of the utility prefix.
                let matched = m.as_str();
                let offset = if matched.starts_with('-') { 0 } else { 1 };
                let col = m.start() + offset;
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "Move the minus inside the brackets (e.g. `top-[-1px]` instead of `-top-[1px]`).".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_negative_prefix_on_top() {
        assert_eq!(run(r#"<div className="-top-[1px]" />"#).len(), 1);
    }

    #[test]
    fn flags_negative_prefix_on_margin() {
        assert_eq!(run(r#"<div className="-mt-[4px] -ml-[2rem]" />"#).len(), 2);
    }

    #[test]
    fn allows_value_inside_brackets() {
        assert!(run(r#"<div className="top-[-1px]" />"#).is_empty());
    }

    #[test]
    fn allows_non_arbitrary_negative_utility() {
        assert!(run(r#"<div className="-m-4 -top-2" />"#).is_empty());
    }

    #[test]
    fn allows_positive_arbitrary() {
        assert!(run(r#"<div className="top-[1px] m-[4px]" />"#).is_empty());
    }

    #[test]
    fn flags_in_vue_class_attribute() {
        assert_eq!(run(r#"<div class="-inset-[2px]" />"#).len(), 1);
    }
}
