mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-electron-node-integration",
    description:
        "`nodeIntegration` in Electron `BrowserWindow`/`BrowserView` exposes Node APIs to renderer \
         content and breaks the sandbox.",
    remediation: "Don't enable nodeIntegration in Electron renderer processes",
    severity: Severity::Error,
    doc_url: Some(
        "https://www.electronjs.org/docs/latest/tutorial/security#2-do-not-enable-nodejs-integration-for-remote-content",
    ),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
