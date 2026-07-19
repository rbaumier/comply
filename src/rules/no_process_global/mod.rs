//! no-process-global — discourage use of the Node `process` global.
//!
//! The concern is runtime portability: production code that relies on the
//! implicit `process` global breaks in browser/edge/Deno runtimes. Test files
//! (`skip_in_test_dir`) always run in Node, where `process` is a legitimate
//! global — spying/mocking it (`vi.spyOn(process, "exit")`, reassigning
//! `process.cwd`) and reading `process.env` for test setup are standard Node
//! idioms, so the portability concern does not apply and they are not flagged.
//!
//! CLI executable entry points are likewise exempt
//! ([`is_cli_entry_point`]): a script with a `#!` shebang, or a file under a
//! `bin/` directory of a package that declares a `bin` field, is invoked
//! directly by Node on the command line. Reading `process.argv`/`process.env`
//! and calling `process.exit()` pervasively is the entry point's whole job —
//! the portability concern (browser/edge/Deno) does not apply to a file that
//! only ever runs as a Node binary.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::project::ProjectCtx;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use std::path::Path;

pub const META: RuleMeta = RuleMeta {
    id: "no-process-global",
    description: "Usage of the Node `process` global is discouraged — it is hard for tools to \
                  statically analyze.",
    remediation: "Import `process` explicitly with `import process from \"node:process\";` instead \
                  of relying on the implicit global.",
    severity: Severity::Error,
    doc_url: Some("https://biomejs.dev/linter/rules/no-process-global/"),
    categories: &["typescript"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

/// True when `path` is a CLI executable entry point, where pervasive use of the
/// `process` global is the file's intended behavior rather than a portability
/// hazard. Two structural signals, both unambiguous markers of a directly-run
/// Node binary:
///
/// - a `#!` shebang first line — the universal marker of an executable script,
///   run as `./script.ts` / `node script.js`, never imported into a browser
///   bundle;
/// - a `bin/` directory segment in a package that declares a `bin` field. The
///   `bin/` segment is the Node convention for CLI sources, and pairing it with
///   the manifest's declared `bin` field keeps an unrelated `bin/` directory in
///   a non-CLI package from matching.
fn is_cli_entry_point(path: &Path, source: &str, project: &ProjectCtx) -> bool {
    if source.starts_with("#!") {
        return true;
    }
    let in_bin_dir = path
        .components()
        .any(|c| matches!(c, std::path::Component::Normal(s) if s == "bin"));
    in_bin_dir
        && project
            .nearest_package_json(path)
            .is_some_and(|pkg| pkg.has_bin)
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
