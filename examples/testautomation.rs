use tracing::Level;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();
    let (_automagic, task) = automagic::start("config.toml");

    task.await.expect("error joining automagic task");
}
