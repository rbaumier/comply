//! llm-intent-naming — does the function name describe intent or implementation?

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
    "verdict": { "type": "string", "enum": ["intent", "implementation", "unclear"] },
    "suggestion": { "type": "string" }
  },
  "required": ["verdict"]
}"#;

fn build_prompt(block: &str) -> String {
    format!(
        r#"Look at this exported function. Does its name describe:
- INTENT: what the operation accomplishes from the caller's perspective (e.g. "closeAccount", "promoteToAdmin", "chargeCustomer")
- IMPLEMENTATION: the storage/mechanical operation being performed (e.g. "setStatusToClosed", "updateRoleField", "insertPaymentRow")

Only flag functions whose names clearly describe implementation. If the name is reasonable, return "intent".

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
    suggestion: Option<String>,
}

impl LlmRule for Rule {
    fn rule_id(&self) -> &'static str { "llm-intent-naming" }
    fn rule_version(&self) -> u32 { RULE_VERSION }

    fn check_block(&self, block: &str, file_path: &Path, block_start_line: usize, model: &str) -> Result<Vec<Diagnostic>> {
        if !block.contains("export ") && !block.contains("pub fn ") {
            return Ok(vec![]);
        }

        let raw = claude_cli::invoke(&claude_cli::LlmRequest {
            prompt: &build_prompt(block),
            json_schema: JSON_SCHEMA,
            model,
        })?;

        let resp: Response = serde_json::from_str(&raw).unwrap_or(Response { verdict: "intent".into(), suggestion: None });

        if resp.verdict != "implementation" {
            return Ok(vec![]);
        }

        let mut msg = "Function name describes implementation, not intent. Rename to express what the operation accomplishes.".to_string();
        if let Some(ref s) = resp.suggestion {
            msg.push_str(&format!(" Suggestion: `{s}`"));
        }

        Ok(vec![Diagnostic {
            path: file_path.to_path_buf(),
            line: block_start_line,
            column: 1,
            rule_id: "llm-intent-naming".into(),
            message: msg,
            severity: Severity::Warning,
        }])
    }
}
