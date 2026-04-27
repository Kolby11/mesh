use crate::PluginRuntimeRole;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Initial handshake from the core to an external plugin host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostHello {
    pub plugin_id: String,
    pub role: PluginRuntimeRole,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub config: HostRuntimeConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostRuntimeConfig {
    #[serde(default)]
    pub dev_mode: bool,
    #[serde(default)]
    pub frontend_entry: Option<String>,
    #[serde(default)]
    pub backend_entry: Option<String>,
}

/// Value exposed by the core as bindable plugin state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindableValue {
    pub id: String,
    #[serde(rename = "type")]
    pub value_type: BindableValueType,
    #[serde(default)]
    pub mutable: bool,
    #[serde(default)]
    pub initial: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BindableValueType {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostRequest {
    RegisterBindable {
        bindable: BindableValue,
    },
    UpdateBindable {
        id: String,
        value: Value,
    },
    SubscribeBindable {
        id: String,
    },
    UnsubscribeBindable {
        id: String,
    },
    InvokeCore {
        command: String,
        #[serde(default)]
        payload: Value,
    },
    EmitEvent {
        channel: String,
        #[serde(default)]
        payload: Value,
    },
    RegisterFrontend {
        component: FrontendComponentRegistration,
    },
    RegisterBackend {
        backend: BackendRegistration,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostEvent {
    Ready {
        hello: HostHello,
    },
    BindableSnapshot {
        values: HashMap<String, Value>,
    },
    BindableChanged {
        id: String,
        value: Value,
    },
    CoreEvent {
        name: String,
        #[serde(default)]
        payload: Value,
    },
    InvokeResult {
        request_id: String,
        #[serde(default)]
        payload: Value,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendComponentRegistration {
    pub surface: String,
    pub framework: String,
    pub entry: String,
    #[serde(default)]
    pub props: HashMap<String, Value>,
    #[serde(default)]
    pub subscribes_to: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendRegistration {
    pub interface: String,
    pub entry: String,
    #[serde(default)]
    pub bindables: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_register_bindable_request() {
        let request = HostRequest::RegisterBindable {
            bindable: BindableValue {
                id: "audio.volume".into(),
                value_type: BindableValueType::Number,
                mutable: true,
                initial: serde_json::json!(42),
            },
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["kind"], "register_bindable");
        assert_eq!(json["bindable"]["id"], "audio.volume");
        assert_eq!(json["bindable"]["type"], "number");
    }
}
