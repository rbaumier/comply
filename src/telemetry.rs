//! Telemetry — a local record of what comply flags.
//! `runs.jsonl` holds one record per lint run.
//! `fps.jsonl` holds each reported false positive.
//! Telemetry is best-effort and never fails a lint.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::changed_lines;
use crate::diagnostic::Diagnostic;

const RUNS_FILE_NAME: &str = "runs.jsonl";
const FALSE_POSITIVES_FILE_NAME: &str = "fps.jsonl";

/// Upper bound on the diagnostics stored per run.
/// It keeps one huge run from bloating the file.
/// `diagnosticsTotal` keeps the truncation visible.
const MAX_RECORDED_DIAGNOSTICS: usize = 500;

/// A snippet exists to recognize the flagged construct.
/// A minified line would drown the record.
const SNIPPET_MAX_CHARS: usize = 160;

/// How many rules and files `comply stats` prints as text.
/// JSON mode emits the full ranking instead.
const TEXT_MODE_TOP_ROWS: usize = 15;

/// One lint run.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// comply-ignore: rust-serde-deny-unknown-fields — an old binary must ignore new keys.
struct RunRecord {
    /// Seconds since the Unix epoch.
    ts: u64,
    repo: String,
    /// The comply invocation, argv minus the binary name.
    args: Vec<String>,
    duration_ms: u64,
    files_linted: usize,
    /// Diagnostics reported, before the recording cap.
    diagnostics_total: usize,
    diagnostics: Vec<DiagnosticRecord>,
}

/// One diagnostic inside a run record.
/// Not [`Diagnostic`]: that is the CI wire format.
/// It denies unknown fields, freezing this schema to it.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// comply-ignore: rust-serde-deny-unknown-fields — same contract as `RunRecord`.
struct DiagnosticRecord {
    path: String,
    line: usize,
    column: usize,
    rule_id: String,
    message: String,
    /// The flagged source line, absent when the file was unreadable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    snippet: Option<String>,
}

/// One false positive, as judged by whoever ran `comply report-fp`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// comply-ignore: rust-serde-deny-unknown-fields — same contract as `RunRecord`.
struct FalsePositiveRecord {
    ts: u64,
    repo: String,
    rule_id: String,
    /// Free-form, `path:line` by convention.
    location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// The model that made the call, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

/// Append a run to `~/.comply/telemetry/runs.jsonl`.
/// Callers treat the error as a warning only.
pub fn record_run(
    diagnostics: &[Diagnostic],
    files_linted: usize,
    duration: Duration,
) -> Result<()> {
    let Some(dir) = telemetry_dir() else {
        eprintln!("comply: telemetry skipped, $HOME is not set");
        return Ok(());
    };
    let record = RunRecord {
        ts: epoch_seconds(),
        repo: current_repo_name(),
        args: std::env::args().skip(1).collect(),
        duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        files_linted,
        diagnostics_total: diagnostics.len(),
        diagnostics: diagnostic_records(diagnostics),
    };
    append_record(&dir, RUNS_FILE_NAME, &record)
}

/// Handle `comply report-fp <RULE_ID> <LOCATION>`.
/// Returns `true` when the input was rejected.
/// The caller turns that into a non-zero exit code.
pub fn report_false_positive(
    rule_id: &str,
    location: &str,
    reason: Option<&str>,
    model: Option<&str>,
) -> Result<bool> {
    if !crate::is_known_rule_id(rule_id) {
        eprintln!(
            "comply: unknown rule: {rule_id}\n\
             Run `comply list` to see all available rule IDs."
        );
        return Ok(true);
    }
    let Some(dir) = telemetry_dir() else {
        eprintln!("comply: cannot record a false positive, $HOME is not set");
        return Ok(true);
    };
    let record = FalsePositiveRecord {
        ts: epoch_seconds(),
        repo: current_repo_name(),
        rule_id: rule_id.to_string(),
        location: location.to_string(),
        reason: reason.map(str::to_string),
        model: model.map(str::to_string),
    };
    append_record(&dir, FALSE_POSITIVES_FILE_NAME, &record)?;
    println!("comply: recorded false positive for {rule_id} at {location}");
    Ok(false)
}

/// Handle `comply stats`.
pub fn run_stats(should_emit_json: bool) -> Result<()> {
    let Some(dir) = telemetry_dir() else {
        eprintln!("comply: no telemetry to read, $HOME is not set");
        return Ok(());
    };
    let runs: Vec<RunRecord> = read_records(&dir, RUNS_FILE_NAME);
    let false_positives: Vec<FalsePositiveRecord> = read_records(&dir, FALSE_POSITIVES_FILE_NAME);

    let has_no_telemetry = runs.is_empty() && false_positives.is_empty();
    if has_no_telemetry && !should_emit_json {
        println!("comply: no telemetry recorded yet");
        return Ok(());
    }

    let stats = aggregate(&runs, &false_positives);
    if should_emit_json {
        let json = serde_json::to_string_pretty(&stats)
            .context("failed to serialize telemetry stats as JSON")?;
        println!("{json}");
    } else {
        print!("{}", render_stats(&stats));
    }
    Ok(())
}

/// `~/.comply/telemetry`, or `None` when `$HOME` is unset.
fn telemetry_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".comply").join("telemetry"))
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since_epoch| since_epoch.as_secs())
}

/// The directory name of the repo being linted.
/// Enough to tell records apart, without storing absolute paths.
fn current_repo_name() -> String {
    changed_lines::git_repo_root()
        .or_else(|| std::env::current_dir().ok())
        .and_then(|root| root.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Serialize one record as a single line, then append it.
/// One `write_all` per record keeps other appenders out.
/// Readers skip whatever still fails to parse.
fn append_record<T: Serialize>(dir: &Path, file_name: &str, record: &T) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let path = dir.join(file_name);
    let mut line = serde_json::to_string(record).context("failed to serialize telemetry record")?;
    line.push('\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to append to {}", path.display()))
}

/// Read every parsable record.
/// A malformed line is dropped silently.
/// It means a concurrent writer, not a user error.
fn read_records<T: DeserializeOwned>(dir: &Path, file_name: &str) -> Vec<T> {
    let Ok(content) = std::fs::read_to_string(dir.join(file_name)) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Convert diagnostics to records, with the flagged source line.
/// Files are grouped, so each is read once.
fn diagnostic_records(diagnostics: &[Diagnostic]) -> Vec<DiagnosticRecord> {
    let capped = &diagnostics[..diagnostics.len().min(MAX_RECORDED_DIAGNOSTICS)];
    let mut lines_by_file: FxHashMap<&Path, Option<Vec<String>>> = FxHashMap::default();
    capped
        .iter()
        .map(|diagnostic| {
            let path = diagnostic.path.as_ref();
            let lines = lines_by_file.entry(path).or_insert_with(|| {
                std::fs::read_to_string(path)
                    .ok()
                    .map(|source| source.lines().map(str::to_string).collect())
            });
            DiagnosticRecord {
                path: path.to_string_lossy().into_owned(),
                line: diagnostic.line,
                column: diagnostic.column,
                rule_id: diagnostic.rule_id.to_string(),
                message: diagnostic.message.clone(),
                snippet: lines
                    .as_deref()
                    .and_then(|lines| snippet_at(lines, diagnostic.line)),
            }
        })
        .collect()
}

/// The 1-indexed `line` of `lines`, trimmed and truncated.
/// Truncation counts characters, so it never splits one.
fn snippet_at(lines: &[String], line: usize) -> Option<String> {
    let raw = lines.get(line.checked_sub(1)?)?;
    Some(raw.trim().chars().take(SNIPPET_MAX_CHARS).collect())
}

/// Everything `comply stats` reports, ranked.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    runs: usize,
    diagnostics: usize,
    false_positives: usize,
    /// Every rule seen, most diagnostics first.
    rules: Vec<RuleStat>,
    /// Every flagged file, most diagnostics first.
    files: Vec<FileStat>,
    /// False positives per reporting model, most first.
    models: Vec<ModelStat>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuleStat {
    rule_id: String,
    count: usize,
    false_positives: usize,
    /// Share of this rule's diagnostics reported as wrong, in percent.
    /// Zero when the rule appears only in `fps.jsonl`.
    false_positive_rate: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileStat {
    path: String,
    count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelStat {
    model: String,
    count: usize,
}

fn aggregate(runs: &[RunRecord], false_positives: &[FalsePositiveRecord]) -> Stats {
    let mut diagnostics_per_rule: FxHashMap<&str, usize> = FxHashMap::default();
    let mut diagnostics_per_file: FxHashMap<&str, usize> = FxHashMap::default();
    for run in runs {
        for diagnostic in &run.diagnostics {
            increment(&mut diagnostics_per_rule, diagnostic.rule_id.as_str());
            increment(&mut diagnostics_per_file, diagnostic.path.as_str());
        }
    }

    let mut false_positives_per_rule: FxHashMap<&str, usize> = FxHashMap::default();
    let mut false_positives_per_model: FxHashMap<&str, usize> = FxHashMap::default();
    for false_positive in false_positives {
        increment(&mut false_positives_per_rule, false_positive.rule_id.as_str());
        increment(
            &mut false_positives_per_model,
            false_positive.model.as_deref().unwrap_or("unspecified"),
        );
    }

    // A reported rule can be absent from the runs.
    // It still gets a row, otherwise its reports vanish.
    let rule_ids: std::collections::BTreeSet<&str> = diagnostics_per_rule
        .keys()
        .chain(false_positives_per_rule.keys())
        .copied()
        .collect();
    let mut rules: Vec<RuleStat> = rule_ids
        .into_iter()
        .map(|rule_id| {
            let count = diagnostics_per_rule.get(rule_id).copied().unwrap_or(0);
            let false_positives = false_positives_per_rule.get(rule_id).copied().unwrap_or(0);
            RuleStat {
                rule_id: rule_id.to_string(),
                count,
                false_positives,
                false_positive_rate: percentage(false_positives, count),
            }
        })
        .collect();
    rules.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.rule_id.cmp(&b.rule_id)));

    let mut files: Vec<FileStat> = diagnostics_per_file
        .into_iter()
        .map(|(path, count)| FileStat { path: path.to_string(), count })
        .collect();
    files.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));

    let mut models: Vec<ModelStat> = false_positives_per_model
        .into_iter()
        .map(|(model, count)| ModelStat { model: model.to_string(), count })
        .collect();
    models.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.model.cmp(&b.model)));

    Stats {
        runs: runs.len(),
        diagnostics: runs.iter().map(|run| run.diagnostics_total).sum(),
        false_positives: false_positives.len(),
        rules,
        files,
        models,
    }
}

/// Count one more occurrence of `key`.
fn increment<'a>(counts: &mut FxHashMap<&'a str, usize>, key: &'a str) {
    let count = counts.entry(key).or_insert(0);
    *count = count.saturating_add(1);
}

fn percentage(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "counts are display-only, and f64 is exact past any plausible count"
    )]
    let rate = (part as f64 / whole as f64) * 100.0;
    rate
}

fn render_stats(stats: &Stats) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "comply telemetry\n  runs         {}\n  diagnostics  {}\n  false pos.   {}\n",
        stats.runs, stats.diagnostics, stats.false_positives
    );

    if !stats.rules.is_empty() {
        let top_rules = &stats.rules[..stats.rules.len().min(TEXT_MODE_TOP_ROWS)];
        let width = top_rules
            .iter()
            .map(|rule| rule.rule_id.len())
            .max()
            .unwrap_or(0);
        let _ = write!(out, "\ntop rules\n");
        let _ = writeln!(out, "  {:<width$}  {:>7}  {:>4}  {:>6}", "rule", "count", "fp", "fp%");
        for rule in top_rules {
            let _ = writeln!(
                out,
                "  {:<width$}  {:>7}  {:>4}  {:>5.1}%",
                rule.rule_id, rule.count, rule.false_positives, rule.false_positive_rate
            );
        }
    }

    if !stats.files.is_empty() {
        let _ = write!(out, "\ntop files\n");
        for file in stats.files.iter().take(TEXT_MODE_TOP_ROWS) {
            let _ = writeln!(out, "  {:>7}  {}", file.count, file.path);
        }
    }

    if !stats.models.is_empty() {
        let _ = write!(out, "\nfalse positives by model\n");
        for model in &stats.models {
            let _ = writeln!(out, "  {:>7}  {}", model.count, model.model);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::sync::Arc;

    fn diagnostic(path: &Path, line: usize, rule_id: &'static str) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column: 1,
            rule_id: rule_id.into(),
            message: "message".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn run_record(diagnostics: Vec<DiagnosticRecord>) -> RunRecord {
        RunRecord {
            ts: 1,
            repo: "comply".into(),
            args: vec!["--working-tree".into()],
            duration_ms: 10,
            files_linted: 1,
            diagnostics_total: diagnostics.len(),
            diagnostics,
        }
    }

    fn diagnostic_record(path: &str, rule_id: &str) -> DiagnosticRecord {
        DiagnosticRecord {
            path: path.into(),
            line: 1,
            column: 1,
            rule_id: rule_id.into(),
            message: "message".into(),
            snippet: None,
        }
    }

    fn false_positive(rule_id: &str, model: Option<&str>) -> FalsePositiveRecord {
        FalsePositiveRecord {
            ts: 1,
            repo: "comply".into(),
            rule_id: rule_id.into(),
            location: "src/main.rs:1".into(),
            reason: None,
            model: model.map(str::to_string),
        }
    }

    #[test]
    fn appended_records_read_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        append_record(dir.path(), RUNS_FILE_NAME, &run_record(vec![])).unwrap();
        append_record(
            dir.path(),
            RUNS_FILE_NAME,
            &run_record(vec![diagnostic_record("src/a.rs", "no-throw")]),
        )
        .unwrap();

        let runs: Vec<RunRecord> = read_records(dir.path(), RUNS_FILE_NAME);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].diagnostics_total, 0);
        assert_eq!(runs[1].diagnostics[0].rule_id, "no-throw");
        assert_eq!(runs[1].args, vec!["--working-tree".to_string()]);
    }

    #[test]
    fn missing_file_reads_as_no_records() {
        let dir = tempfile::tempdir().unwrap();
        let runs: Vec<RunRecord> = read_records(dir.path(), RUNS_FILE_NAME);
        assert!(runs.is_empty());
    }

    #[test]
    fn malformed_line_is_skipped_and_neighbours_survive() {
        // A concurrent appender can leave a torn line.
        // It must not cost us the rest of the file.
        let dir = tempfile::tempdir().unwrap();
        append_record(dir.path(), RUNS_FILE_NAME, &run_record(vec![])).unwrap();
        let path = dir.path().join(RUNS_FILE_NAME);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"ts\":1,\"repo\":\n").unwrap();
        append_record(dir.path(), RUNS_FILE_NAME, &run_record(vec![])).unwrap();

        let runs: Vec<RunRecord> = read_records(dir.path(), RUNS_FILE_NAME);
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn unknown_field_does_not_drop_the_record() {
        // A record written by a newer comply stays readable here.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FALSE_POSITIVES_FILE_NAME);
        std::fs::write(
            &path,
            "{\"ts\":1,\"repo\":\"r\",\"ruleId\":\"no-throw\",\"location\":\"a:1\",\"verdict\":\"x\"}\n",
        )
        .unwrap();

        let records: Vec<FalsePositiveRecord> = read_records(dir.path(), FALSE_POSITIVES_FILE_NAME);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].rule_id, "no-throw");
    }

    #[test]
    fn stats_joins_false_positives_onto_their_rule() {
        let runs = vec![run_record(vec![
            diagnostic_record("src/a.rs", "no-throw"),
            diagnostic_record("src/a.rs", "no-throw"),
            diagnostic_record("src/a.rs", "no-throw"),
            diagnostic_record("src/a.rs", "no-throw"),
            diagnostic_record("src/b.rs", "id-length"),
        ])];
        let false_positives = vec![
            false_positive("no-throw", Some("opus")),
            false_positive("id-length", Some("opus")),
            false_positive("id-length", None),
        ];

        let stats = aggregate(&runs, &false_positives);

        assert_eq!(stats.runs, 1);
        assert_eq!(stats.diagnostics, 5);
        assert_eq!(stats.false_positives, 3);
        // Ranked by diagnostic count: no-throw has 4, id-length 1.
        assert_eq!(stats.rules[0].rule_id, "no-throw");
        assert_eq!(stats.rules[0].count, 4);
        assert_eq!(stats.rules[0].false_positives, 1);
        assert!((stats.rules[0].false_positive_rate - 25.0).abs() < f64::EPSILON);
        // More reports than recorded diagnostics reads as over 100%.
        assert_eq!(stats.rules[1].rule_id, "id-length");
        assert!((stats.rules[1].false_positive_rate - 200.0).abs() < f64::EPSILON);
        assert_eq!(stats.files[0].path, "src/a.rs");
        assert_eq!(stats.files[0].count, 4);
        assert_eq!(stats.models[0].model, "opus");
        assert_eq!(stats.models[0].count, 2);
        assert_eq!(stats.models[1].model, "unspecified");
    }

    #[test]
    fn stats_keeps_a_rule_reported_only_as_a_false_positive() {
        let stats = aggregate(&[], &[false_positive("no-throw", None)]);
        assert_eq!(stats.rules[0].rule_id, "no-throw");
        assert_eq!(stats.rules[0].count, 0);
        assert_eq!(stats.rules[0].false_positives, 1);
        assert!(stats.rules[0].false_positive_rate.abs() < f64::EPSILON);
    }

    #[test]
    fn snippet_is_the_flagged_line_trimmed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.rs");
        std::fs::write(&path, "fn a() {}\n    let x = 1;\nfn b() {}\n").unwrap();

        let records = diagnostic_records(&[diagnostic(&path, 2, "id-length")]);

        assert_eq!(records[0].snippet.as_deref(), Some("let x = 1;"));
        assert_eq!(records[0].line, 2);
    }

    #[test]
    fn snippet_truncates_on_character_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.rs");
        // Multi-byte characters: a byte-wise cut would split one.
        let long_line = "é".repeat(400);
        std::fs::write(&path, format!("{long_line}\n")).unwrap();

        let records = diagnostic_records(&[diagnostic(&path, 1, "id-length")]);

        let snippet = records[0].snippet.as_deref().unwrap();
        assert_eq!(snippet.chars().count(), SNIPPET_MAX_CHARS);
        assert!(long_line.starts_with(snippet));
    }

    #[test]
    fn snippet_is_absent_for_an_unreadable_file() {
        let records = diagnostic_records(&[diagnostic(Path::new("/nope/missing.rs"), 1, "x")]);
        assert!(records[0].snippet.is_none());

        let json = serde_json::to_string(&records[0]).unwrap();
        assert!(!json.contains("snippet"));
    }

    #[test]
    fn snippet_is_absent_past_the_end_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.rs");
        std::fs::write(&path, "fn a() {}\n").unwrap();

        let records = diagnostic_records(&[diagnostic(&path, 99, "id-length")]);
        assert!(records[0].snippet.is_none());
    }

    #[test]
    fn recorded_diagnostics_are_capped_but_the_total_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.rs");
        std::fs::write(&path, "let x = 1;\n").unwrap();
        let diagnostics: Vec<Diagnostic> = (0..MAX_RECORDED_DIAGNOSTICS + 42)
            .map(|_| diagnostic(&path, 1, "id-length"))
            .collect();

        let records = diagnostic_records(&diagnostics);

        assert_eq!(records.len(), MAX_RECORDED_DIAGNOSTICS);
        assert_eq!(diagnostics.len(), MAX_RECORDED_DIAGNOSTICS + 42);
    }

    #[test]
    fn rendered_stats_show_counts_and_rates() {
        let stats = aggregate(
            &[run_record(vec![diagnostic_record("src/a.rs", "no-throw")])],
            &[false_positive("no-throw", Some("opus"))],
        );
        let text = render_stats(&stats);
        assert!(text.contains("no-throw"));
        assert!(text.contains("100.0%"));
        assert!(text.contains("src/a.rs"));
        assert!(text.contains("opus"));
    }
}
