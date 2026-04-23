use std::process::ExitCode;

use appctl::cli::Cli;
use clap::Parser;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    if let Err(err) = cli.run().await {
        eprintln!("{err:#}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
