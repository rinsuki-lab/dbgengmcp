use clap::Parser as _;
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};

mod mcp;
mod windbg;

#[derive(clap::Parser)]
struct Cli {
    #[arg(long)]
    bind: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let service = StreamableHttpService::new(
        || Ok(mcp::DebuggerService::new()),
        LocalSessionManager::default().into(),
        Default::default(),
    );
    axum::serve(
        tokio::net::TcpListener::bind(&cli.bind).await.unwrap(),
        axum::Router::new().nest_service("/mcp", service),
    )
    .await
    .unwrap();
}
