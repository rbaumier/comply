//! Mandatory external toolchains.
//!
//! comply delegates part of its rule set to programs it does not own: clippy,
//! cargo-shear, oxlint, and the typescript-go checker behind the
//! type-aware sidecar. None is optional. Skipping one would silently
//! under-report, so comply refuses to run rather than print a false "clean".
//!
//! A refusal is a diagnosis, not a crash. This type is what every refusal
//! raises, so `main` can answer with the remedy instead of a bug-report banner.

/// A toolchain comply requires but cannot use.
#[derive(Debug, thiserror::Error)]
pub enum ToolchainError {
    /// Absent, and one command installs it.
    #[error("{what}.\nInstall it with: {install_cmd}")]
    Missing { what: String, install_cmd: String },

    /// A gap comply can name but no single command closes.
    /// An interpreter with no one-line install, a checker API that won't start.
    #[error("{what}.")]
    Unusable { what: String },
}

/// What becomes of files a pass could not analyze.
#[derive(Debug, Clone, Copy)]
pub enum Unanalyzed {
    /// A git file scope refuses them, with exit code 2.
    Refuse,
    /// A full scan notes them on stderr.
    Note,
}

/// How many unanalyzed files a message names.
const NAMED_FILES: usize = 20;

/// Names the `files` a pass did not analyze, and why.
pub fn report_unanalyzed(
    why: &str,
    files: &[&crate::files::SourceFile],
    unanalyzed: Unanalyzed,
) -> anyhow::Result<()> {
    let names: String = files
        .iter()
        .take(NAMED_FILES)
        .map(|file| format!("\n  {}", file.path.display()))
        .collect();
    let more = files.len().saturating_sub(NAMED_FILES);
    let rest = if more == 0 {
        String::new()
    } else {
        format!("\n  and {more} more")
    };
    // Paths first, one per line; the reason takes the dot.
    let what = format!("these file(s) were not analyzed:{names}{rest}\n{why}");
    match unanalyzed {
        Unanalyzed::Refuse => Err(ToolchainError::Unusable { what }.into()),
        Unanalyzed::Note => {
            eprintln!("comply: {what}.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_renders_the_install_command_on_its_own_line() {
        let err = ToolchainError::Missing {
            what: "oxlint is required but was not found".to_owned(),
            install_cmd: "npm install -g oxlint".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "oxlint is required but was not found.\nInstall it with: npm install -g oxlint"
        );
    }

    #[test]
    fn unusable_renders_a_single_sentence() {
        let err = ToolchainError::Unusable {
            what: "`node` was not found".to_owned(),
        };
        assert_eq!(err.to_string(), "`node` was not found.");
    }

    /// The message `report_unanalyzed` refuses `count` files with.
    fn refusal(count: usize) -> String {
        let files: Vec<crate::files::SourceFile> = (1..=count)
            .map(|n| crate::files::SourceFile {
                path: format!("src/f{n}.rs").into(),
                language: crate::files::Language::Rust,
            })
            .collect();
        let refs: Vec<&crate::files::SourceFile> = files.iter().collect();
        let Err(error) = report_unanalyzed("a pass failed", &refs, Unanalyzed::Refuse) else {
            panic!("{count} file(s) passed");
        };
        error.to_string()
    }

    #[test]
    fn a_refusal_names_up_to_twenty_files() {
        let bounds = [
            NAMED_FILES.saturating_sub(1),
            NAMED_FILES,
            NAMED_FILES.saturating_add(1),
        ];
        for count in bounds {
            let message = refusal(count);
            let named = message
                .lines()
                .filter(|line| line.starts_with("  src/"))
                .count();
            assert_eq!(named, count.min(NAMED_FILES), "{message}");
            assert_eq!(
                message.contains("and 1 more"),
                count > NAMED_FILES,
                "{message}"
            );
        }
    }

    #[test]
    fn a_refusal_ends_on_its_reason_not_on_a_path() {
        let message = refusal(2);
        assert!(message.ends_with("\na pass failed."), "{message}");
        assert!(
            message.lines().any(|line| line == "  src/f2.rs"),
            "{message}"
        );
    }
}
