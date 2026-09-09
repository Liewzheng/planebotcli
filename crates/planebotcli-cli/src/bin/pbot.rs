//! `pbot` binary — thin alias entry sharing `planebotcli_cli`.

use clap::Parser;
use planebotcli_cli::{Cli, run};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("Error: {}", err);
        if let Some(hint) = err.hint() {
            eprintln!("Hint: {hint}");
        }
        std::process::exit(err.exit_code());
    }
}
