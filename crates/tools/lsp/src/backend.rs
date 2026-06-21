use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;
use tower_lsp::{Client, LanguageServer, jsonrpc::Result, lsp_types::*};

use crate::{
    analyzer, diagnostics, document::Document, hover, manifest, manifest::ManifestDocument,
    module_registry::ModuleRegistry, semantic_tokens,
};

pub struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, Document>>>,
    manifests: Arc<RwLock<HashMap<Url, ManifestDocument>>>,
    registry: Arc<RwLock<ModuleRegistry>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            manifests: Arc::new(RwLock::new(HashMap::new())),
            registry: Arc::new(RwLock::new(ModuleRegistry::empty())),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Discover modules from the workspace root provided by the client.
        let workspace_root = params
            .root_uri
            .as_ref()
            .and_then(|uri| uri.to_file_path().ok());

        if let Some(root) = workspace_root {
            let registry = ModuleRegistry::discover(&root);
            tracing::info!(
                modules = registry.manifests.len(),
                services = registry.interface_fields.len(),
                "module registry built"
            );
            *self.registry.write().await = registry;
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "mesh-tools-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "<".into(),
                        " ".into(),
                        ".".into(),
                        "\"".into(),
                        "{".into(),
                        ":".into(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(semantic_tokens::server_capabilities()),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("mesh-tools-lsp ready");
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let source = params.text_document.text;
        self.update_document(uri, source).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // Full sync — use the first (only) change which contains the complete text.
        if let Some(change) = params.content_changes.into_iter().next() {
            self.update_document(uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        self.manifests.write().await.remove(&uri);
        // Clear diagnostics on close.
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        if manifest::is_manifest_uri(uri) {
            let manifests = self.manifests.read().await;
            let Some(doc) = manifests.get(uri) else {
                return Ok(None);
            };
            let items = manifest::complete::complete(doc, position);
            return Ok((!items.is_empty()).then_some(CompletionResponse::Array(items)));
        }

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };
        let registry = self.registry.read().await;

        let items = analyzer::complete(doc, position, &registry);
        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        if manifest::is_manifest_uri(uri) {
            let manifests = self.manifests.read().await;
            let Some(doc) = manifests.get(uri) else {
                return Ok(None);
            };
            return Ok(manifest::hover::hover(doc, position));
        }

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };
        let registry = self.registry.read().await;

        Ok(hover::hover(doc, position, &registry))
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        // Manifest documents (module.json/package.json) are not `.mesh` files;
        // leave them to a JSON formatter.
        if manifest::is_manifest_uri(uri) {
            return Ok(None);
        }

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };

        let indent_unit = if params.options.insert_spaces {
            " ".repeat(params.options.tab_size.max(1) as usize)
        } else {
            "\t".to_string()
        };

        let formatted = crate::format::format_document(&doc.source, &indent_unit);
        if formatted == doc.source {
            return Ok(None);
        }

        Ok(Some(vec![TextEdit {
            range: full_document_range(&doc.source),
            new_text: formatted,
        }]))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };

        Ok(Some(semantic_tokens::full(doc)))
    }
}

impl Backend {
    async fn update_document(&self, uri: Url, source: String) {
        if manifest::is_manifest_uri(&uri) {
            let doc = ManifestDocument::new(uri.clone(), source);
            let diags = manifest::diagnostics::diagnostics(&doc);
            self.manifests.write().await.insert(uri.clone(), doc);
            self.client.publish_diagnostics(uri, diags, None).await;
            return;
        }

        let doc = Document::new(uri.clone(), source);
        let diags = diagnostics::from_document(&doc);
        self.documents.write().await.insert(uri.clone(), doc);
        self.client.publish_diagnostics(uri, diags, None).await;
    }
}

/// A range that spans the entire document, for whole-document replacement edits.
fn full_document_range(source: &str) -> Range {
    let mut last_line = 0u32;
    let mut last_line_len = 0u32;
    for ch in source.chars() {
        if ch == '\n' {
            last_line += 1;
            last_line_len = 0;
        } else {
            last_line_len += ch.len_utf16() as u32;
        }
    }
    Range {
        start: Position::new(0, 0),
        end: Position::new(last_line, last_line_len),
    }
}
