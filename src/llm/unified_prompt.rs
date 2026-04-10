//! Unified LLM prompt — sends ONE `claude -p` call per file instead of
//! one per rule. The single prompt asks Claude to evaluate ALL semantic
//! criteria at once and return a structured JSON response.
//!
//! Why unified: each `claude -p` subprocess has ~15s startup overhead
//! (Node.js init + auth). 4 rules × 15s = 60s overhead per file.
//! A unified prompt pays the overhead once.

use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};
use super::claude_cli;

/// Run the unified prompt on one file and return all diagnostics.
pub fn evaluate_file(
    source: &str,
    file_path: &Path,
    model: &str,
) -> Result<Vec<Diagnostic>> {
    if source.lines().count() < 3 {
        return Ok(vec![]);
    }
    let prompt = build_prompt(source);
    let raw = claude_cli::invoke(&claude_cli::LlmRequest {
        prompt: &prompt,
        json_schema: UNIFIED_SCHEMA,
        model,
    })?;
    let resp: UnifiedResponse = serde_json::from_str(&raw)
        .unwrap_or_else(|_| UnifiedResponse {
            comment_quality: CommentQuality::default(),
            intent_naming: vec![],
            pii_in_logs: vec![],
            mixed_abstraction: vec![],
        });
    convert_response(resp, file_path)
}

const UNIFIED_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "comment_quality": {
      "type": "object",
      "properties": {
        "issues": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "line": { "type": "integer" },
              "criterion": { "type": "string" },
              "explanation": { "type": "string" },
              "suggestion": { "type": "string" }
            },
            "required": ["criterion", "explanation"]
          }
        }
      },
      "required": ["issues"]
    },
    "intent_naming": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "line": { "type": "integer" },
          "function_name": { "type": "string" },
          "suggestion": { "type": "string" }
        },
        "required": ["function_name"]
      }
    },
    "pii_in_logs": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "line": { "type": "integer" },
          "fields": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["fields"]
      }
    },
    "mixed_abstraction": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "line": { "type": "integer" },
          "function_name": { "type": "string" },
          "high_level": { "type": "string" },
          "low_level": { "type": "string" }
        },
        "required": ["function_name"]
      }
    }
  },
  "required": ["comment_quality", "intent_naming", "pii_in_logs", "mixed_abstraction"]
}"#;

fn build_prompt(source: &str) -> String {
    format!(
        r#"You are a code quality auditor. Analyze the following source file and evaluate it against these 4 categories. Be strict but fair — only flag real problems, not pedantic nitpicks.

## 1. COMMENT QUALITY
Evaluate every comment against these criteria:
- Does it answer "what goes wrong if I delete this?" (names a consequence, not a paraphrase)
- Chains cause → effect ("Advance cursor so the next tick skips these rows")
- Technical terms explained on first use (what it is, why it exists)
- Structs/types describe ROLE not fields ("All data the state machine needs" NOT "Contains failure_percent")
- Concrete over abstract (specific numbers/thresholds, not "retry with backoff")
- Active voice, complete sentences, emphatic word at end
- NOT a label ("// cache expiry 30m") — must be prose
- NOT a paraphrase of the function/variable name

Flag: paraphrases_code, no_consequence, abstract_not_concrete, passive_voice, describes_fields_not_role, mechanical_tone, label_not_prose, missing_cause_effect

## 2. INTENT NAMING
For each exported/public function: does the name describe INTENT (closeAccount, chargeCustomer) or IMPLEMENTATION (setStatusToClosed, updateRoleField)?
Only flag clear implementation names.

## 3. PII IN LOGS
For each logger/console call: does it output PII? (email, phone, SSN, password, API key, credit card, IP address, date of birth, full name)

## 4. MIXED ABSTRACTION LEVELS
For functions >20 lines: does it mix high-level orchestration (calling well-named functions) with low-level detail (regex, byte manipulation, raw string parsing)?

Return EMPTY arrays for categories with no issues. Be conservative — false negatives are better than false positives.

Source file:
```
{source}
```"#
    )
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct UnifiedResponse {
    #[serde(default)]
    comment_quality: CommentQuality,
    #[serde(default)]
    intent_naming: Vec<IntentIssue>,
    #[serde(default)]
    pii_in_logs: Vec<PiiIssue>,
    #[serde(default)]
    mixed_abstraction: Vec<AbstractionIssue>,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize, Default)]
struct CommentQuality {
    #[serde(default)]
    issues: Vec<CommentIssue>,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct CommentIssue {
    #[serde(default)]
    line: Option<usize>,
    criterion: String,
    explanation: String,
    #[serde(default)]
    suggestion: Option<String>,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct IntentIssue {
    #[serde(default)]
    line: Option<usize>,
    function_name: String,
    #[serde(default)]
    suggestion: Option<String>,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct PiiIssue {
    #[serde(default)]
    line: Option<usize>,
    fields: Vec<String>,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct AbstractionIssue {
    #[serde(default)]
    line: Option<usize>,
    function_name: String,
    #[serde(default)]
    high_level: Option<String>,
    #[serde(default)]
    low_level: Option<String>,
}

fn convert_response(resp: UnifiedResponse, file_path: &Path) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    // Comment quality issues.
    for issue in &resp.comment_quality.issues {
        let line = issue.line.unwrap_or(1);
        let mut msg = format!(
            "Comment quality: {} — {}",
            issue.criterion.replace('_', " "),
            issue.explanation,
        );
        if let Some(ref s) = issue.suggestion
            && !s.is_empty() {
                msg.push_str(&format!(" Rewrite: {s}"));
            }
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line, column: 1,
            rule_id: "llm-comment-quality".into(),
            message: msg,
            severity: Severity::Warning,
        });
    }

    // Intent naming issues.
    for issue in &resp.intent_naming {
        let line = issue.line.unwrap_or(1);
        let mut msg = format!(
            "Function `{}` describes implementation, not intent.",
            issue.function_name,
        );
        if let Some(ref s) = issue.suggestion {
            msg.push_str(&format!(" Suggestion: `{s}`"));
        }
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line, column: 1,
            rule_id: "llm-intent-naming".into(),
            message: msg,
            severity: Severity::Warning,
        });
    }

    // PII in logs.
    for issue in &resp.pii_in_logs {
        let line = issue.line.unwrap_or(1);
        let fields = issue.fields.join(", ");
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line, column: 1,
            rule_id: "llm-pii-in-logs".into(),
            message: format!("PII in log output: {fields}. Redact before logging."),
            severity: Severity::Error,
        });
    }

    // Mixed abstraction.
    for issue in &resp.mixed_abstraction {
        let line = issue.line.unwrap_or(1);
        let mut msg = format!(
            "Function `{}` mixes abstraction levels.",
            issue.function_name,
        );
        if let (Some(hi), Some(lo)) = (&issue.high_level, &issue.low_level) {
            msg = format!(
                "Function `{}` mixes abstraction levels: high ({hi}) with low ({lo}). Extract the low-level detail.",
                issue.function_name,
            );
        }
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line, column: 1,
            rule_id: "llm-function-abstraction-levels".into(),
            message: msg,
            severity: Severity::Warning,
        });
    }

    Ok(diagnostics)
}
