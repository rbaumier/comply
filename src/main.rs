mod cli;

use clap::Parser;
use cli::Cli;

fn main() {
    let cli = Cli::parse();
    let _mode = cli.scan_mode();
    println!("comply: scan mode resolved");
}
