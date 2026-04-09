mod cli;
mod diagnostic;
mod files;
mod output;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use files::Language;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mode = cli.scan_mode();
    let discovered = files::discover(&mode)?;

    let ts_count = discovered
        .iter()
        .filter(|f| f.language == Language::TypeScript)
        .count();
    let rs_count = discovered
        .iter()
        .filter(|f| f.language == Language::Rust)
        .count();

    println!("comply: found {ts_count} TS/JS files, {rs_count} Rust files");
    Ok(())
}
