use airframe_args::ArgsModuleWithStartup;
use airframe_core::app::AppBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(ArgsModuleWithStartup::new())
        .start()
        .await?;

    if let Some(args) = app.services.get::<airframe_args::CliArgs>() {
        println!("argv len = {}", args.argv.len());
        if let Some(cmd) = &args.command {
            println!("command: {}", cmd);
        }
    }

    app.cancel.cancel();
    Ok(())
}
