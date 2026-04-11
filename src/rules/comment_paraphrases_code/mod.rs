//! comment-paraphrases-code — flag comments that restate the code they sit on.
//!
//! From the coding-standards skill: "every comment answers what goes wrong
//! if I delete this? If you can't name a consequence, the comment is a
//! paraphrase". A paraphrase comment is worse than no comment — it adds
//! visual noise without information and rots when the code changes.
//!
//! This rule is heuristic — it WILL produce some false positives on
//! genuinely informative comments that happen to share vocabulary with
//! the function name. Severity is `Warning` and the rule is opt-out via
//! `comply-ignore: comment-paraphrases-code — <justification>`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "comment-paraphrases-code",
    description: "Comment shares too many tokens with the function name — likely a paraphrase.",
    remediation: "Rewrite the comment to explain WHY the code exists, not WHAT it does. \
                  Name the consequence: what breaks if this line is deleted? If you \
                  can't name a consequence, delete the comment instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
