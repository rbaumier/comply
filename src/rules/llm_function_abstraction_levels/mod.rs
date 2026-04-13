//! llm-function-abstraction-levels — detect mixed abstraction levels in functions.

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
    "verdict": { "type": "string", "enum": ["clean", "mixed"] },
    "high_level": { "type": "string" },
    "low_level": { "type": "string" }
  },
  "required": ["verdict"]
}"#;

fn build_prompt(block: &str) -> String {
    format!(
        r#"Examine this function. Does it mix high-level orchestration with low-level details?

HIGH-LEVEL: calling other well-named functions, coordinating steps, business logic flow
LOW-LEVEL: regex patterns, byte manipulation, string parsing, manual iteration, raw SQL construction, bit operations

A function at ONE level of abstraction is clean. A function that calls `validateOrder(order)` then inlines a regex to parse a date string is MIXED.

Only flag genuinely mixed functions. Short utility functions that ARE the low-level detail are fine.

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
    high_level: Option<String>,
    #[serde(default)]
    low_level: Option<String>,
}

impl LlmRule for Rule {
    fn rule_id(&self) -> &'static str { "llm-function-abstraction-levels" }
    fn rule_version(&self) -> u32 { RULE_VERSION }

    fn check_block(&self, block: &str, file_path: &Path, block_start_line: usize, model: &str) -> Result<Vec<Diagnostic>> {
        // Only check substantial functions.
        if block.lines().count() < 20 { return Ok(vec![]); }

        let raw = claude_cli::invoke(&claude_cli::LlmRequest {
            prompt: &build_prompt(block),
            json_schema: JSON_SCHEMA,
            model,
        })?;

        let resp: Response = serde_json::from_str(&raw).unwrap_or(Response { verdict: "clean".into(), high_level: None, low_level: None });

        if resp.verdict != "mixed" { return Ok(vec![]); }

        let mut msg = "Function mixes abstraction levels. Extract the low-level detail into a named helper.".to_string();
        if let (Some(hi), Some(lo)) = (&resp.high_level, &resp.low_level) {
            msg = format!("Function mixes abstraction levels: high-level ({hi}) interleaved with low-level ({lo}). Extract the low-level detail into a named helper.");
        }

        Ok(vec![Diagnostic {
            path: file_path.to_path_buf(),
            line: block_start_line,
            column: 1,
            rule_id: "llm-function-abstraction-levels".into(),
            message: msg,
            severity: Severity::Warning,
            span: None,
        }])
    }
}
