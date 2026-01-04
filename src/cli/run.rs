//! Run command implementation

use clap::Args;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

impl RunArgs {
    pub async fn execute(&self) -> anyhow::Result<()> {
        // TODO: Implement paper trading loop
        tracing::info!("Starting paper trading...");
        Ok(())
    }
}
