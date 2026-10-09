//! palantour — sovereign festival application intelligence.
//! One binary, one artist, one embedded database.

use pt_cli::{Cli, Command};
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    pt_cli::run(cli).await
}
