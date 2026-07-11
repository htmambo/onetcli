use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let socket_name = std::env::var("OMNIHUB_IPC_SOCKET")
        .or_else(|_| std::env::var("OMNIHUB_DUCKDB_DRIVER_SOCKET"))
        .ok()
        .or_else(|| std::env::args().nth(1))
        .unwrap_or_else(|| "omnihub-duckdb-driver.sock".to_string());

    duckdb_driver::server::run(&socket_name).await
}
