use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::{Mutex, RwLock};
use tower_lsp::{Client, LanguageServer, jsonrpc::Result, lsp_types::*};

use crate::{
    analyzer, definition, diagnostics, document::Document, hover, manifest,
    manifest::ManifestDocument, module_registry::ModuleRegistry, semantic_tokens, settings,
    settings::SettingsDocument,
};

pub struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, Document>>>,
    manifests: Arc<RwLock<HashMap<Url, ManifestDocument>>>,
    settings: Arc<RwLock<HashMap<Url, SettingsDocument>>>,
    registry: Arc<RwLock<ModuleRegistry>>,
    workspace_root: Arc<RwLock<Option<PathBuf>>>,
    refresh_generation: Arc<RwLock<u64>>,
    document_versions: Arc<RwLock<HashMap<Url, i32>>>,
    document_update_lock: Arc<Mutex<()>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            manifests: Arc::new(RwLock::new(HashMap::new())),
            settings: Arc::new(RwLock::new(HashMap::new())),
            registry: Arc::new(RwLock::new(ModuleRegistry::empty())),
            workspace_root: Arc::new(RwLock::new(None)),
            refresh_generation: Arc::new(RwLock::new(0)),
            document_versions: Arc::new(RwLock::new(HashMap::new())),
            document_update_lock: Arc::new(Mutex::new(())),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Prefer the legacy root URI when it is usable, but accept clients
        // that provide only the current workspace-folder field.
        let workspace_root = workspace_root_from_initialize(&params);

        if let Some(root) = workspace_root {
            *self.workspace_root.write().await = Some(root);
            let generation = self.start_refresh_generation().await;
            self.refresh_registry(generation).await;
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "mesh-tools-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                // Every source offset is translated to the LSP's UTF-16
                // coordinate space at the protocol boundary.
                position_encoding: Some(PositionEncodingKind::UTF16),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
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
                definition_provider: Some(OneOf::Left(true)),
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
        let version = params.text_document.version;
        if let Some(generation) = self.update_document(uri, source, version).await {
            self.refresh_registry(generation).await;
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        // Full sync — use the first (only) change which contains the complete text.
        if let Some(change) = params.content_changes.into_iter().next() {
            if let Some(generation) = self.update_document(uri, change.text, version).await {
                self.refresh_registry(generation).await;
            }
        }
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        let generation = self.start_refresh_generation().await;
        self.refresh_registry(generation).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let _update_guard = self.document_update_lock.lock().await;
        self.documents.write().await.remove(&uri);
        self.manifests.write().await.remove(&uri);
        self.settings.write().await.remove(&uri);
        self.document_versions.write().await.remove(&uri);
        drop(_update_guard);
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
            let items = manifest::complete(doc, position);
            return Ok((!items.is_empty()).then_some(CompletionResponse::Array(items)));
        }

        if settings::is_settings_uri(uri) {
            let settings_docs = self.settings.read().await;
            let Some(doc) = settings_docs.get(uri) else {
                return Ok(None);
            };
            let registry = self.registry.read().await;
            let items = settings::complete(doc, position, &registry);
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
            return Ok(manifest::hover(doc, position));
        }

        if settings::is_settings_uri(uri) {
            let settings_docs = self.settings.read().await;
            let Some(doc) = settings_docs.get(uri) else {
                return Ok(None);
            };
            let registry = self.registry.read().await;
            return Ok(settings::hover(doc, position, &registry));
        }

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };
        let registry = self.registry.read().await;

        Ok(hover::hover(doc, position, &registry))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };
        let registry = self.registry.read().await;

        Ok(definition::definition(doc, position, &registry))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        // Manifest documents (module.json) are not `.mesh` files;
        // leave them to a JSON formatter.
        if manifest::is_manifest_uri(uri) || settings::is_settings_uri(uri) {
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
    async fn start_refresh_generation(&self) -> u64 {
        let mut generation = self.refresh_generation.write().await;
        *generation = generation.saturating_add(1);
        *generation
    }

    async fn refresh_registry(&self, generation: u64) {
        let root = self.workspace_root.read().await.clone();
        let Some(root) = root else {
            return;
        };
        match ModuleRegistry::try_discover(&root) {
            Ok(registry) => {
                tracing::info!(
                    modules = registry.manifests.len(),
                    services = registry.interface_fields.len(),
                    revision = ?registry.snapshot_revision(),
                    "canonical authoring snapshot refreshed"
                );
                if !self.commit_registry(generation, registry).await {
                    tracing::debug!(generation, "discarding stale authoring snapshot refresh");
                }
            }
            Err(error) => {
                tracing::warn!(
                    workspace = %root.display(),
                    "keeping previous authoring snapshot after refresh failure: {error}"
                );
            }
        }
    }

    async fn commit_registry(&self, generation: u64, next: ModuleRegistry) -> bool {
        // Hold the generation read lock while taking the registry write lock.
        // A newer notification cannot advance the generation between this
        // check and the publication of `next`.
        let current_generation = self.refresh_generation.read().await;
        if *current_generation != generation {
            return false;
        }
        *self.registry.write().await = next;
        true
    }

    async fn update_document(&self, uri: Url, source: String, version: i32) -> Option<u64> {
        if manifest::is_manifest_uri(&uri) {
            let doc = ManifestDocument::new(uri.clone(), source);
            let diags = manifest::diagnostics(&doc);
            let _update_guard = self.document_update_lock.lock().await;
            if !self.commit_document_version(&uri, version).await {
                return None;
            }
            self.manifests.write().await.insert(uri.clone(), doc);
            drop(_update_guard);
            let generation = self.start_refresh_generation().await;
            self.client
                .publish_diagnostics(uri, diags, Some(version))
                .await;
            return Some(generation);
        }

        if settings::is_settings_uri(&uri) {
            let doc = SettingsDocument::new(uri.clone(), source);
            let diags = {
                let registry = self.registry.read().await;
                settings::diagnostics(&doc, &registry)
            };
            let _update_guard = self.document_update_lock.lock().await;
            if !self.commit_document_version(&uri, version).await {
                return None;
            }
            self.settings.write().await.insert(uri.clone(), doc);
            drop(_update_guard);
            let generation = self.start_refresh_generation().await;
            self.client
                .publish_diagnostics(uri, diags, Some(version))
                .await;
            return Some(generation);
        }

        let doc = Document::new(uri.clone(), source);
        let diags = diagnostics::from_document(&doc);
        let _update_guard = self.document_update_lock.lock().await;
        if !self.commit_document_version(&uri, version).await {
            return None;
        }
        self.documents.write().await.insert(uri.clone(), doc);
        drop(_update_guard);
        let generation = self.start_refresh_generation().await;
        self.client
            .publish_diagnostics(uri, diags, Some(version))
            .await;
        Some(generation)
    }

    async fn commit_document_version(&self, uri: &Url, version: i32) -> bool {
        let mut versions = self.document_versions.write().await;
        if versions.get(uri).is_some_and(|current| *current >= version) {
            return false;
        }
        versions.insert(uri.clone(), version);
        true
    }
}

fn workspace_root_from_initialize(params: &InitializeParams) -> Option<PathBuf> {
    params
        .root_uri
        .as_ref()
        .and_then(|uri| uri.to_file_path().ok())
        .or_else(|| {
            params
                .workspace_folders
                .as_ref()?
                .iter()
                .find_map(|folder| folder.uri.to_file_path().ok())
        })
}

/// A range that spans the entire document, for whole-document replacement edits.
fn full_document_range(source: &str) -> Range {
    Range::new(
        Position::new(0, 0),
        crate::util::offset_to_position(source, source.len()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::LspService;

    #[test]
    fn full_document_range_uses_utf16_end_position() {
        assert_eq!(
            full_document_range("é😀\nnext"),
            Range::new(Position::new(0, 0), Position::new(1, 4))
        );
    }

    #[test]
    fn workspace_folder_is_used_when_root_uri_is_missing() {
        let params = InitializeParams {
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::parse("file:///workspace").unwrap(),
                name: "workspace".into(),
            }]),
            ..Default::default()
        };

        assert_eq!(
            workspace_root_from_initialize(&params),
            Some(PathBuf::from("/workspace"))
        );
    }

    #[test]
    fn usable_root_uri_takes_precedence_over_workspace_folder() {
        let params = InitializeParams {
            root_uri: Some(Url::parse("file:///root").unwrap()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::parse("file:///workspace").unwrap(),
                name: "workspace".into(),
            }]),
            ..Default::default()
        };

        assert_eq!(
            workspace_root_from_initialize(&params),
            Some(PathBuf::from("/root"))
        );
    }

    #[tokio::test]
    async fn stale_refresh_generation_cannot_replace_newer_registry() {
        let (service, _) = LspService::new(Backend::new);
        let backend = service.inner();
        let first_generation = backend.start_refresh_generation().await;
        let second_generation = backend.start_refresh_generation().await;

        let mut stale = ModuleRegistry::empty();
        stale.themes.push("stale".into());
        assert!(!backend.commit_registry(first_generation, stale).await);

        let mut current = ModuleRegistry::empty();
        current.themes.push("current".into());
        assert!(backend.commit_registry(second_generation, current).await);
        assert_eq!(
            backend.registry.read().await.themes,
            vec!["current".to_string()]
        );
    }

    #[tokio::test]
    async fn older_document_versions_are_rejected() {
        let (service, _) = LspService::new(Backend::new);
        let backend = service.inner();
        let uri = Url::parse("file:///workspace/main.mesh").unwrap();

        assert!(backend.commit_document_version(&uri, 2).await);
        assert!(!backend.commit_document_version(&uri, 1).await);
        assert!(!backend.commit_document_version(&uri, 2).await);
        assert!(backend.commit_document_version(&uri, 3).await);
    }
}
