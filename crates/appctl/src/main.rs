use anyhow::Result;
use appctl::cli::Cli;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    Cli::parse().run().await
}
