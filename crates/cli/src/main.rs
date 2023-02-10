use telemetry::TelemetrySubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    TelemetrySubscriber::init(std::io::stdout)?;

    cli::run().await?;

    Ok(())
}
