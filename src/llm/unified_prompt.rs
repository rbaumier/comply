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

/// Parse raw JSON from the Bun worker into diagnostics.
pub fn parse_response(raw: &str, file_path: &Path) -> Result<Vec<Diagnostic>> {
    let resp: UnifiedResponse = serde_json::from_str(raw).unwrap_or_else(|_| UnifiedResponse {
        comment_quality: CommentQuality::default(),
        intent_naming: vec![],
        pii_in_logs: vec![],
        mixed_abstraction: vec![],
        define_errors_out_of_existence: vec![],
        pull_complexity_downward: vec![],
        barricade_pattern: vec![],
        temporal_decomposition: vec![],
        shallow_module: vec![],
        parse_dont_validate: vec![],
        invalid_states_unrepresentable: vec![],
        functional_core_imperative_shell: vec![],
        document_impossible_states: vec![],
        bound_every_input: vec![],
        crosscutting_via_wrapping: vec![],
        map_db_entities_to_dtos: vec![],
        error_messages_as_remediation: vec![],
    });
    convert_response(resp, file_path)
}

#[allow(dead_code)]
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
    "define_errors_out_of_existence": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "line": { "type": "integer" },
          "function_name": { "type": "string" },
          "error_condition": { "type": "string" },
          "redesign": { "type": "string" }
        },
        "required": ["function_name", "error_condition"]
      }
    },
    "pull_complexity_downward": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "line": { "type": "integer" },
          "function_name": { "type": "string" },
          "pushed_complexity": { "type": "string" }
        },
        "required": ["function_name", "pushed_complexity"]
      }
    },
    "barricade_pattern": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "line": { "type": "integer" },
          "function_name": { "type": "string" },
          "explanation": { "type": "string" }
        },
        "required": ["function_name", "explanation"]
      }
    },
    "temporal_decomposition": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "line": { "type": "integer" },
          "module_or_function": { "type": "string" },
          "steps": { "type": "string" },
          "hidden_decision": { "type": "string" }
        },
        "required": ["module_or_function", "steps"]
      }
    },
    "shallow_module": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "line": { "type": "integer" },
          "function_name": { "type": "string" },
          "explanation": { "type": "string" }
        },
        "required": ["function_name", "explanation"]
      }
    }
  },
  "required": ["comment_quality", "intent_naming", "pii_in_logs", "mixed_abstraction", "define_errors_out_of_existence", "pull_complexity_downward", "barricade_pattern", "temporal_decomposition", "shallow_module"]
}"#;

pub fn build_prompt(source: &str) -> String {
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

## 5. DEFINE ERRORS OUT OF EXISTENCE
For each function that throws/returns an error on a condition the caller cannot prevent: could the API be redesigned so the error condition cannot arise? Example: `unset(key)` succeeds even if key is absent (guarantees "after call, key doesn't exist"). This is NOT ignoring errors — it's designing better contracts.
Only flag functions where a clear redesign exists. Do NOT flag genuine failure modes (network, IO, auth).

## 6. PULL COMPLEXITY DOWNWARD
For each public function or module interface: does it push hard decisions to callers via config params, exceptions, or "you figure it out" interfaces? A module has more users than developers — better for the implementer to suffer once than for every caller to suffer repeatedly.
Flag: config parameter that could be a decision inside the module, exception thrown for an uncertain condition that could be handled internally, return value requiring complex interpretation.

## 7. BARRICADE PATTERN (SCATTERED VALIDATION)
Is input validation scattered throughout inner logic instead of concentrated at trust boundaries? Look for the same validation check (null guard, type check, range check, format validation) repeated in multiple internal functions. Validation should happen ONCE at the module boundary; inner code should trust clean inputs.
Do NOT flag security-critical checks (auth, crypto, PII) — those are intentionally redundant.

## 8. TEMPORAL DECOMPOSITION
Are modules/functions split by execution order (read → parse → validate → store) instead of by information hiding? Each step then knows about the data format, coupling them tightly. Flag when module boundaries mirror execution steps rather than encapsulate design decisions.

## 9. SHALLOW MODULE
Is a function's public interface as complex as its implementation? Flag pass-through methods that forward calls with identical signatures adding no value. Flag wrapper functions whose body is a single delegation. A module's interface should be simpler than its implementation.

## 10. PARSE DON'T VALIDATE
Flag functions that validate data and then pass raw untyped values downstream. The validated data should be parsed into a typed representation (newtype, branded type, or validated struct) so the type system guarantees validity. Flag: `if (isEmail(str)) sendEmail(str)` — `str` is still `string` after validation.

## 11. INVALID STATES UNREPRESENTABLE
Flag types where invalid combinations of fields are possible at the type level. Example: `{{ status: 'pending', completedAt: Date }}` — a pending item with a completion date. Suggest discriminated unions or state machines.

## 12. FUNCTIONAL CORE IMPERATIVE SHELL
Flag functions that mix pure business logic with I/O (database calls, HTTP requests, file system, logging). The pure logic should be extractable into a function that takes data in and returns data out, with I/O at the edges.

## 13. DOCUMENT IMPOSSIBLE STATES
Flag assertions, panics, or unreachable branches that lack a comment explaining why the state is impossible. Every `assert`, `unreachable!`, `throw new Error('unreachable')` needs a `// Impossible because: ...` comment.

## 14. BOUND EVERY INPUT
Flag public function parameters from external boundaries (API handlers, CLI args, config parsing, user input) that are used without validation. External inputs must be validated and rejected if invalid — never silently defaulted with `?? 0` or `|| fallback`.

## 15. CROSSCUTTING VIA WRAPPING
Flag `logger.info()`, `metrics.record()`, `tracer.span()` calls inside business logic functions. Crosscutting concerns (logging, tracing, metrics) should be injected via wrapping (`withLogging(service)`) not inlined.

## 16. MAP DB ENTITIES TO DTOS
Flag route handlers or API functions that return raw database entities (Prisma models, TypeORM entities, Drizzle query results) directly. Database entities should be mapped to dedicated response DTOs at the boundary.

## 17. ERROR MESSAGES AS REMEDIATION
Flag `new Error("...")` or `throw new Error("...")` where the message just names the problem without telling the reader what to do. Good: `"API key expired. Generate a new one at /settings/api-keys"`. Bad: `"Invalid API key"`.

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
    #[serde(default)]
    define_errors_out_of_existence: Vec<DefineErrorsIssue>,
    #[serde(default)]
    pull_complexity_downward: Vec<PullComplexityIssue>,
    #[serde(default)]
    barricade_pattern: Vec<BarricadeIssue>,
    #[serde(default)]
    temporal_decomposition: Vec<TemporalIssue>,
    #[serde(default)]
    shallow_module: Vec<ShallowModuleIssue>,
    #[serde(default)]
    parse_dont_validate: Vec<GenericIssue>,
    #[serde(default)]
    invalid_states_unrepresentable: Vec<GenericIssue>,
    #[serde(default)]
    functional_core_imperative_shell: Vec<GenericIssue>,
    #[serde(default)]
    document_impossible_states: Vec<GenericIssue>,
    #[serde(default)]
    bound_every_input: Vec<GenericIssue>,
    #[serde(default)]
    crosscutting_via_wrapping: Vec<GenericIssue>,
    #[serde(default)]
    map_db_entities_to_dtos: Vec<GenericIssue>,
    #[serde(default)]
    error_messages_as_remediation: Vec<GenericIssue>,
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

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct DefineErrorsIssue {
    #[serde(default)]
    line: Option<usize>,
    function_name: String,
    error_condition: String,
    #[serde(default)]
    redesign: Option<String>,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct PullComplexityIssue {
    #[serde(default)]
    line: Option<usize>,
    function_name: String,
    pushed_complexity: String,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct BarricadeIssue {
    #[serde(default)]
    line: Option<usize>,
    function_name: String,
    explanation: String,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct TemporalIssue {
    #[serde(default)]
    line: Option<usize>,
    module_or_function: String,
    steps: String,
    #[serde(default)]
    hidden_decision: Option<String>,
}

/// External wire format mirror — LLM JSON output.
#[derive(Debug, Deserialize)]
struct ShallowModuleIssue {
    #[serde(default)]
    line: Option<usize>,
    function_name: String,
    explanation: String,
}

#[derive(Debug, Deserialize)]
struct GenericIssue {
    #[serde(default)]
    line: Option<usize>,
    #[serde(default)]
    explanation: String,
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
            && !s.is_empty()
        {
            msg.push_str(&format!(" Rewrite: {s}"));
        }
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line,
            column: 1,
            rule_id: "llm-comment-quality".into(),
            message: msg,
            severity: Severity::Warning,
            span: None,
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
            line,
            column: 1,
            rule_id: "llm-intent-naming".into(),
            message: msg,
            severity: Severity::Warning,
            span: None,
        });
    }

    // PII in logs.
    for issue in &resp.pii_in_logs {
        let line = issue.line.unwrap_or(1);
        let fields = issue.fields.join(", ");
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line,
            column: 1,
            rule_id: "llm-pii-in-logs".into(),
            message: format!("PII in log output: {fields}. Redact before logging."),
            severity: Severity::Error,
            span: None,
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
            line,
            column: 1,
            rule_id: "llm-function-abstraction-levels".into(),
            message: msg,
            severity: Severity::Warning,
            span: None,
        });
    }

    // Define errors out of existence.
    for issue in &resp.define_errors_out_of_existence {
        let line = issue.line.unwrap_or(1);
        let mut msg = format!(
            "Function `{}` throws/returns error on `{}` — redesign the API so this condition cannot arise.",
            issue.function_name, issue.error_condition,
        );
        if let Some(ref r) = issue.redesign {
            msg.push_str(&format!(" Suggestion: {r}"));
        }
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line,
            column: 1,
            rule_id: "llm-define-errors-out-of-existence".into(),
            message: msg,
            severity: Severity::Warning,
            span: None,
        });
    }

    // Pull complexity downward.
    for issue in &resp.pull_complexity_downward {
        let line = issue.line.unwrap_or(1);
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line, column: 1,
            rule_id: "llm-pull-complexity-downward".into(),
            message: format!(
                "Function `{}` pushes complexity to callers: {}. Absorb this decision inside the module.",
                issue.function_name, issue.pushed_complexity,
            ),
            severity: Severity::Warning,
            span: None,
        });
    }

    // Barricade pattern (scattered validation).
    for issue in &resp.barricade_pattern {
        let line = issue.line.unwrap_or(1);
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line, column: 1,
            rule_id: "llm-barricade-pattern".into(),
            message: format!(
                "Function `{}`: {}. Move validation to the module boundary; inner code should trust clean inputs.",
                issue.function_name, issue.explanation,
            ),
            severity: Severity::Warning,
            span: None,
        });
    }

    // Temporal decomposition.
    for issue in &resp.temporal_decomposition {
        let line = issue.line.unwrap_or(1);
        let mut msg = format!(
            "`{}` is split by execution order ({}), not by information hiding.",
            issue.module_or_function, issue.steps,
        );
        if let Some(ref h) = issue.hidden_decision {
            msg.push_str(&format!(
                " Hidden decision that should define the boundary: {h}"
            ));
        }
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line,
            column: 1,
            rule_id: "llm-temporal-decomposition".into(),
            message: msg,
            severity: Severity::Warning,
            span: None,
        });
    }

    // Shallow module.
    for issue in &resp.shallow_module {
        let line = issue.line.unwrap_or(1);
        diagnostics.push(Diagnostic {
            path: file_path.to_path_buf(),
            line, column: 1,
            rule_id: "llm-shallow-module".into(),
            message: format!(
                "Function `{}` is a shallow wrapper: {}. A module's interface should be simpler than its implementation.",
                issue.function_name, issue.explanation,
            ),
            severity: Severity::Warning,
            span: None,
        });
    }

    // Generic issue categories (Tier 5 LLM rules).
    let generic_categories = [
        ("llm-parse-dont-validate", &resp.parse_dont_validate),
        (
            "llm-invalid-states-unrepresentable",
            &resp.invalid_states_unrepresentable,
        ),
        (
            "llm-functional-core-imperative-shell",
            &resp.functional_core_imperative_shell,
        ),
        (
            "llm-document-impossible-states",
            &resp.document_impossible_states,
        ),
        ("llm-bound-every-input", &resp.bound_every_input),
        (
            "llm-crosscutting-via-wrapping",
            &resp.crosscutting_via_wrapping,
        ),
        ("llm-map-db-entities-to-dtos", &resp.map_db_entities_to_dtos),
        (
            "llm-error-messages-as-remediation",
            &resp.error_messages_as_remediation,
        ),
    ];
    for (rule_id, issues) in &generic_categories {
        for issue in *issues {
            let line = issue.line.unwrap_or(1);
            if !issue.explanation.is_empty() {
                diagnostics.push(Diagnostic {
                    path: file_path.to_path_buf(),
                    line,
                    column: 1,
                    rule_id: (*rule_id).into(),
                    message: issue.explanation.clone(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }

    Ok(diagnostics)
}
