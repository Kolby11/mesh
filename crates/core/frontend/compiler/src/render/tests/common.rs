use super::super::*;
use crate::TemplateExpressionResult;
use mesh_core_elements::{ComponentCompositionProps, EventHandlerCall};
use std::collections::BTreeMap;

pub(super) struct TranslatingStore;

impl VariableStore for TranslatingStore {
    fn get(&self, _name: &str) -> Option<serde_json::Value> {
        None
    }

    fn keys(&self) -> Vec<String> {
        Vec::new()
    }

    fn translate(&self, key: &str) -> Option<String> {
        (key == "nav.open_settings").then(|| "Open settings".to_string())
    }
}

pub(super) struct IdentityTranslationComposition;

impl FrontendCompositionResolver for IdentityTranslationComposition {
    fn evaluate_template_expression(
        &self,
        _instance_key: &str,
        expression: &mesh_core_expression::CompiledExpression,
        _locals: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<TemplateExpressionResult> {
        let _ = expression;
        None
    }

    fn render_import(
        &self,
        _host: &Manifest,
        _host_instance_key: &str,
        _owner_source_path: Option<&std::path::Path>,
        _alias: &str,
        _source_ordinal: usize,
        _duplicate_ordinal: Option<usize>,
        _repeated_by_loop: bool,
        _loop_identity: Option<&str>,
        _props: &ComponentCompositionProps,
        _prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
        _container_width: f32,
        _container_height: f32,
    ) -> Option<WidgetNode> {
        None
    }

    fn render_slot(
        &self,
        _host: &Manifest,
        _host_instance_key: &str,
        _extension_point: Option<&str>,
        _slot_name: Option<&str>,
        _customizable: bool,
        _container_width: f32,
        _container_height: f32,
    ) -> Vec<WidgetNode> {
        Vec::new()
    }
}

pub(super) struct TypedExpressionComposition;

impl FrontendCompositionResolver for TypedExpressionComposition {
    fn evaluate_template_expression(
        &self,
        _instance_key: &str,
        expression: &mesh_core_expression::CompiledExpression,
        _locals: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<TemplateExpressionResult> {
        let value = match expression.source() {
            "enabled" => serde_json::json!(true),
            "minimum" => serde_json::json!(1.5),
            "metadata" => serde_json::json!({"source": "runtime"}),
            _ => serde_json::Value::Null,
        };
        Some(TemplateExpressionResult {
            value,
            service_reads: Vec::new(),
        })
    }

    fn render_import(
        &self,
        _host: &Manifest,
        _host_instance_key: &str,
        _owner_source_path: Option<&std::path::Path>,
        _alias: &str,
        _source_ordinal: usize,
        _duplicate_ordinal: Option<usize>,
        _repeated_by_loop: bool,
        _loop_identity: Option<&str>,
        _props: &ComponentCompositionProps,
        _prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
        _container_width: f32,
        _container_height: f32,
    ) -> Option<WidgetNode> {
        None
    }

    fn render_slot(
        &self,
        _host: &Manifest,
        _host_instance_key: &str,
        _extension_point: Option<&str>,
        _slot_name: Option<&str>,
        _customizable: bool,
        _container_width: f32,
        _container_height: f32,
    ) -> Vec<WidgetNode> {
        Vec::new()
    }
}

pub(super) struct MapStore(pub(super) std::collections::HashMap<String, serde_json::Value>);

pub(super) fn test_manifest() -> Manifest {
    Manifest {
        package: mesh_core_module::ModuleSection {
            id: "test".into(),
            version: "0.1.0".into(),
            module_type: mesh_core_module::ModuleType::Widget,
            api_version: "0.1.0".into(),
            name: None,
            license: None,
            description: None,
            authors: vec![],
            repository: None,
        },
        compatibility: Default::default(),
        dependencies: Default::default(),
        capabilities: Default::default(),
        entrypoints: Default::default(),
        accessibility: None,
        keybinds: Default::default(),
        i18n: None,
        theme: None,
        service: None,
        provides: vec![],
        interface: None,
        interfaces: Vec::new(),
        extensions: vec![],
        exports: Default::default(),
        hosted_extension_points: Default::default(),
        extension_point_contributions: Default::default(),
        surface_layout: None,
        assets: None,
        icons: None,
        icon_pack: None,
        icon_requirements: Default::default(),
        translations: Default::default(),
    }
}
