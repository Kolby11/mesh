use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "mesh_tools_lsp=info".to_string()),
        )
        .with_writer(std::io::stderr)
        .init();

    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(mesh_tools_lsp::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
