//! llm-pii-in-logs — detect PII in logger/console calls.

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
    "verdict": { "type": "string", "enum": ["safe", "pii"] },
    "fields": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["verdict"]
}"#;

fn build_prompt(block: &str) -> String {
    format!(
        r#"Examine this code for PII (Personally Identifiable Information) being logged.

PII includes: email addresses, phone numbers, full names, physical addresses, SSN/national ID, passwords, API keys/tokens, credit card numbers, IP addresses, dates of birth.

Look for logger/console calls (console.log, console.error, logger.info, log::info, tracing::info, println, eprintln, etc.) that output PII fields.

If no logging calls exist or no PII is logged, return "safe".

Code:
```
{block}
```"#
    )
}

#[derive(Debug, Deserialize)]
/// External wire format mirror — LLM JSON output.
struct Response {
    verdict: String,
    #[serde(default)]
    fields: Vec<String>,
}

impl LlmRule for Rule {
    fn rule_id(&self) -> &'static str { "llm-pii-in-logs" }
    fn rule_version(&self) -> u32 { RULE_VERSION }

    fn check_block(&self, block: &str, file_path: &Path, block_start_line: usize, model: &str) -> Result<Vec<Diagnostic>> {
        let has_log = ["console.", "logger.", "log::", "tracing::", "println!", "eprintln!", "log."]
            .iter().any(|p| block.contains(p));
        if !has_log { return Ok(vec![]); }

        let raw = claude_cli::invoke(&claude_cli::LlmRequest {
            prompt: &build_prompt(block),
            json_schema: JSON_SCHEMA,
            model,
        })?;

        let resp: Response = serde_json::from_str(&raw).unwrap_or(Response { verdict: "safe".into(), fields: vec![] });

        if resp.verdict != "pii" { return Ok(vec![]); }

        let fields = if resp.fields.is_empty() { "unknown".to_string() } else { resp.fields.join(", ") };
        Ok(vec![Diagnostic {
            path: file_path.to_path_buf(),
            line: block_start_line,
            column: 1,
            rule_id: "llm-pii-in-logs".into(),
            message: format!("PII detected in log output: {fields}. Remove PII fields or redact before logging."),
            severity: Severity::Error,
            span: None,
        }])
    }
}
