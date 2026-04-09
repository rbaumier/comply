//! Language Server Protocol implementation: `comply lsp` runs comply
//! as an LSP server on stdio so editors can show diagnostics inline
//! while the user types.
//!
//! Scope (intentional limits):
//!   - Only the in-process tree-sitter and text rules run. Oxlint and
//!     clippy are subprocess-based; spawning them on every keystroke
//!     would freeze the editor. Users still get them on the next
//!     `comply` CLI run (e.g. on save via an editor task).
//!   - Diagnostics are recomputed on every `didOpen` / `didChange` /
//!     `didSave`. There's no incremental parsing yet — comply re-runs
//!     the rule pass on the full document text. With ~80 rules and
//!     tree-sitter parsing being microseconds-fast, this is well
//!     under the editor's perceptual threshold for files up to a
//!     few thousand lines.
//!   - Config is loaded once at `initialize` from the workspace root.
//!     Editing `comply.toml` mid-session does not reload it — restart
//!     the LSP server. Live config reload is a v2.11 nicety.
//!
//! Wire format: stdio is the canonical LSP transport. Editors that
//! prefer TCP can wrap us in `socat` if needed; we don't ship a TCP
//! mode because every editor we care about supports stdio.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    InitializeParams, InitializeResult, InitializedParams, MessageType, Position, Range,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::config::Config;
use crate::diagnostic::{Diagnostic as ComplyDiagnostic, Severity};
use crate::engine;
use crate::files::Language;

/// Server state shared between LSP request handlers. The Client lets
/// us push notifications back to the editor (publishDiagnostics, log
/// messages); the Config is loaded once at `initialize` and held
/// behind an RwLock so a future "reload config" command can swap it
/// in without restarting the server.
struct Backend {
    client: Client,
    config: RwLock<Arc<Config>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        // Resolve the workspace root from the editor's initialize
        // params and load `comply.toml` from there. We prefer
        // workspace_folders (modern LSP), fall back to root_uri
        // (legacy), and finally to the cwd.
        let anchor = workspace_anchor(&params)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        match Config::load_from(&anchor) {
            Ok(cfg) => {
                *self.config.write().await = Arc::new(cfg);
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("comply: loaded config from {}", anchor.display()),
                    )
                    .await;
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("comply: config load failed ({e:#}); using defaults"),
                    )
                    .await;
            }
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "comply".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                // We need the full document text on every change so we
                // can re-lint it. Incremental sync would require us to
                // maintain a rope and apply diffs, which isn't worth
                // it for files small enough that a full re-lint is
                // already sub-millisecond.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "comply LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.publish_for(&params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // With FULL sync the editor always sends us the entire new
        // text in a single `content_changes` entry — pull it out and
        // re-lint.
        if let Some(change) = params.content_changes.into_iter().next() {
            self.publish_for(&params.text_document.uri, &change.text)
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // The save notification carries the post-save text only when
        // the editor opted in via TextDocumentSaveRegistrationOptions
        // with `include_text = true`. We don't request that, so the
        // best we can do here is fall back to the disk read — but the
        // didChange handler already pushed fresh diagnostics from the
        // in-memory text, so didSave is essentially a no-op for us.
        let _ = params;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Clear our diagnostics for the closed document so the editor
        // doesn't keep showing stale squiggles after the buffer is gone.
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }
}

impl Backend {
    /// Run the in-memory lint pass on `text` and push the resulting
    /// LSP diagnostics for the document at `uri`. Skips files whose
    /// extension comply doesn't recognize so we don't pollute the
    /// editor with empty `publishDiagnostics` for unrelated buffers.
    async fn publish_for(&self, uri: &Url, text: &str) {
        let Some(path) = uri_to_path(uri) else { return };
        let Some(language) = Language::from_path(&path) else {
            return;
        };
        let cfg = self.config.read().await.clone();
        let diagnostics = engine::lint_in_memory(&path, language, text, &cfg);
        let lsp_diagnostics: Vec<LspDiagnostic> =
            diagnostics.iter().map(comply_to_lsp_diagnostic).collect();
        self.client
            .publish_diagnostics(uri.clone(), lsp_diagnostics, None)
            .await;
    }
}

/// Spin up the LSP server on stdio. Blocks until the editor closes
/// the channel; called from `main` when the user runs `comply lsp`.
pub async fn run() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        config: RwLock::new(Arc::new(Config::default())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Pull the workspace root out of the LSP `initialize` params,
/// preferring the modern `workspace_folders` field over the legacy
/// `root_uri`. Returns `None` when the editor passed neither — the
/// caller falls back to the cwd.
fn workspace_anchor(params: &InitializeParams) -> Option<PathBuf> {
    if let Some(folders) = &params.workspace_folders
        && let Some(first) = folders.first()
    {
        return uri_to_path(&first.uri);
    }
    #[allow(deprecated)]
    if let Some(root) = &params.root_uri {
        return uri_to_path(root);
    }
    None
}

/// Convert a `file://` URL into a local filesystem path. Returns
/// `None` for non-file schemes (`untitled:`, `vscode:`, etc.) so the
/// LSP gracefully ignores buffers it can't lint.
fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    if uri.scheme() != "file" {
        return None;
    }
    uri.to_file_path().ok()
}

/// Translate a comply Diagnostic to the LSP wire format. We map
/// 1-based (line, column) coordinates back to LSP's 0-based and use
/// a one-character range — the editor highlights the column where
/// comply pointed, which is precise enough for the rules we have.
fn comply_to_lsp_diagnostic(d: &ComplyDiagnostic) -> LspDiagnostic {
    let line = d.line.saturating_sub(1) as u32;
    let column = d.column.saturating_sub(1) as u32;
    LspDiagnostic {
        range: Range {
            start: Position { line, character: column },
            end: Position { line, character: column + 1 },
        },
        severity: Some(match d.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(d.rule_id.clone())),
        source: Some("comply".to_string()),
        message: d.message.clone(),
        ..LspDiagnostic::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comply_diag_maps_to_one_indexed_lsp_position() {
        let d = ComplyDiagnostic {
            path: PathBuf::from("/tmp/x.ts"),
            line: 10,
            column: 5,
            rule_id: "no-throw".into(),
            message: "uses throw".into(),
            severity: Severity::Error,
        };
        let lsp = comply_to_lsp_diagnostic(&d);
        assert_eq!(lsp.range.start.line, 9);
        assert_eq!(lsp.range.start.character, 4);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn line_zero_does_not_underflow() {
        let d = ComplyDiagnostic {
            path: PathBuf::from("/tmp/x.ts"),
            line: 0,
            column: 0,
            rule_id: "x".into(),
            message: "y".into(),
            severity: Severity::Warning,
        };
        let lsp = comply_to_lsp_diagnostic(&d);
        assert_eq!(lsp.range.start.line, 0);
        assert_eq!(lsp.range.start.character, 0);
    }

    #[test]
    fn non_file_scheme_returns_none() {
        let url = Url::parse("untitled:Untitled-1").unwrap();
        assert!(uri_to_path(&url).is_none());
    }
}
