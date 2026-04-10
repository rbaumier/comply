//! llm-comment-quality — comprehensive comment evaluation via LLM.
//!
//! Evaluates every comment block against ALL criteria from the
//! coding-standards skill: why & consequences, domain explanation,
//! tone & style, anti-patterns. This is the rule that catches what
//! no regex or AST walk can: "is this comment actually useful?"

use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};
use crate::llm::{claude_cli, LlmRule};

#[derive(Debug)]
pub struct Rule;

const RULE_VERSION: u32 = 1;

const JSON_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "verdict": { "type": "string", "enum": ["good", "needs_improvement"] },
    "issues": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "criterion": { "type": "string", "enum": [
            "paraphrases_code", "no_consequence", "abstract_not_concrete",
            "passive_voice", "describes_fields_not_role", "no_domain_explanation",
            "mechanical_tone", "wrong_emphasis_order", "missing_cause_effect",
            "label_not_prose", "missing_return_contract", "missing_gotcha_warning"
          ]},
          "explanation": { "type": "string" }
        },
        "required": ["criterion", "explanation"]
      }
    },
    "rewrite_suggestion": { "type": "string" }
  },
  "required": ["verdict", "issues"]
}"#;

fn build_prompt(code_block: &str) -> String {
    format!(
        r#"You are a code comment quality auditor. Evaluate the comments in the following code block against these criteria:

## WHY & CONSEQUENCES
- Every comment must answer "what goes wrong if I delete this?" If it can't name a consequence, it's a paraphrase.
- Chain cause → effect: "Advance cursor so the next tick skips these rows"
- Inaction must be justified: empty branches, no-ops, early returns need a "why not"

## CONCEPTS & DOMAIN
- Technical terms explained on first use (what it is, what it does, why it exists, how it connects)
- Plain language first, jargon in parentheses: "Find postings that have no matching entry (dangling postings)"
- Structs/types: describe the ROLE, not the fields. "All the data the state machine needs to decide" NOT "Contains failure_percent, last_status"
- State transitions: past tense for what happened, present for conclusion
- Limits/invariants: WHY the cap + what happens to leftovers

## TONE & STYLE
- Conversational, not mechanical: "where did I stop last time?" not "reads the cursor"
- Concrete: specific numbers, names, thresholds. "Retry 3 times with 500ms backoff" not "Retry with backoff"
- Complete sentences, active voice, capitalized
- Emphatic word at end: the WHY lands last. "Skip validation — already checked upstream" not "Already checked, so skip"

## ANTI-PATTERNS TO FLAG
- Paraphrase: comment restates the function/variable name
- Label not prose: "// cache expiry 30m" instead of a sentence
- Passive voice: "Failed jobs are retried" → "The scheduler retries failed jobs"
- Abstract without concrete: no numbers, no names
- Fields listed instead of role
- Error messages that don't tell the user what to DO

If there are NO comments in the code, or the code block is too short to evaluate meaningfully, return verdict "good" with empty issues.

Only flag real problems. Don't be pedantic about short utility functions with obvious names.

Code block:
```
{code_block}
```"#
    )
}

#[derive(Debug, Deserialize)]
/// External wire format mirror — LLM JSON output.
struct Response {
    verdict: String,
    #[serde(default)]
    issues: Vec<Issue>,
    #[serde(default)]
    rewrite_suggestion: Option<String>,
}

#[derive(Debug, Deserialize)]
/// External wire format mirror — LLM JSON output.
struct Issue {
    criterion: String,
    explanation: String,
}

impl LlmRule for Rule {
    fn rule_id(&self) -> &'static str {
        "llm-comment-quality"
    }

    fn rule_version(&self) -> u32 {
        RULE_VERSION
    }

    fn check_block(
        &self,
        block: &str,
        file_path: &Path,
        block_start_line: usize,
        model: &str,
    ) -> Result<Vec<Diagnostic>> {
        // Skip tiny files / blocks with no comments.
        if block.lines().count() < 5 || !block.contains("//") && !block.contains("/*") {
            return Ok(vec![]);
        }

        let prompt = build_prompt(block);
        let raw = claude_cli::invoke(&claude_cli::LlmRequest {
            prompt: &prompt,
            json_schema: JSON_SCHEMA,
            model,
        })?;

        let resp: Response = serde_json::from_str(&raw)
            .unwrap_or_else(|_| Response {
                verdict: "good".into(),
                issues: vec![],
                rewrite_suggestion: None,
            });

        if resp.verdict == "good" || resp.issues.is_empty() {
            return Ok(vec![]);
        }

        let mut diagnostics = Vec::new();
        for issue in &resp.issues {
            let mut msg = format!(
                "Comment quality: {} — {}",
                issue.criterion.replace('_', " "),
                issue.explanation,
            );
            if let Some(ref rewrite) = resp.rewrite_suggestion
                && !rewrite.is_empty() && diagnostics.is_empty() {
                    msg.push_str(&format!(" Suggested rewrite: {rewrite}"));
                }
            diagnostics.push(Diagnostic {
                path: file_path.to_path_buf(),
                line: block_start_line,
                column: 1,
                rule_id: "llm-comment-quality".into(),
                message: msg,
                severity: Severity::Warning,
            });
        }

        Ok(diagnostics)
    }
}
