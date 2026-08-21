use semver::{Version, VersionReq};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// A parsed interface contract.
///
/// Contracts are declared inline in `module.json`, or in a module-relative
/// JSON file referenced by an interface declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceContract {
    pub interface: String,
    pub version: Version,
    /// Documented core state fields that providers must emit. These are read
    /// through the service proxy as plain field access (e.g. `audio.percent`)
    /// and are never callable methods.
    pub state_fields: Vec<ContractStateField>,
    /// Mutating command methods callable from frontend scripts. Read-style
    /// accessors are NOT included here — they must use `state_fields` instead.
    pub methods: Vec<InterfaceMethod>,
    pub events: Vec<InterfaceEvent>,
    pub types: HashMap<String, InterfaceTypeDef>,
    pub capabilities: ContractCapabilities,
}

/// Where an interface declaration came from. A missing value is intentional:
/// contracts assembled by tests or older callers have no file-backed source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeclarationProvenance {
    pub module: Option<String>,
    pub source: Option<String>,
}

impl DeclarationProvenance {
    pub fn unknown() -> Self {
        Self::default()
    }

    pub fn new(module: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            module: Some(module.into()),
            source: Some(source.into()),
        }
    }
}

/// A normalized field in a compiled schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledField {
    pub name: String,
    pub type_expr: TypeExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledStateField {
    pub name: String,
    pub type_expr: TypeExpr,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledMethodSchema {
    pub name: String,
    pub args: Arc<[CompiledField]>,
    pub returns: Option<TypeExpr>,
    pub coalesce: bool,
    pub state_binding: Option<StateBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledEventSchema {
    pub name: String,
    pub payload: Arc<[CompiledField]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledTypeSchema {
    pub name: String,
    pub fields: Arc<[CompiledField]>,
}

/// The complete recursive schema after all expressions have been parsed.
/// `BTreeMap` gives the artifact deterministic ordering independent of JSON
/// object insertion order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSchemas {
    pub state_fields: Arc<[CompiledStateField]>,
    pub methods: Arc<[CompiledMethodSchema]>,
    pub events: Arc<[CompiledEventSchema]>,
    pub types: Arc<BTreeMap<String, CompiledTypeSchema>>,
}

/// Capability policy after legacy fallback and ordering normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledOperationPolicy {
    pub read: Option<Arc<[String]>>,
    pub events: Arc<BTreeMap<String, Arc<[String]>>>,
    pub methods: Arc<BTreeMap<String, Arc<[String]>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledMethodBehavior {
    pub coalesce: bool,
    pub state_binding: Option<StateBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledBehavioralMetadata {
    pub methods: Arc<BTreeMap<String, CompiledMethodBehavior>>,
}

/// The immutable runtime/tooling artifact for one canonical contract version.
/// Raw declarations remain available through `to_interface_contract()` for
/// compatibility with authoring and migration consumers, while all runtime
/// decisions should use the normalized schema and policy fields here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledContract {
    pub(crate) raw: Arc<InterfaceContract>,
    pub interface: String,
    pub version: Version,
    pub state_fields: Arc<[ContractStateField]>,
    pub methods: Arc<[InterfaceMethod]>,
    pub events: Arc<[InterfaceEvent]>,
    pub types: Arc<HashMap<String, InterfaceTypeDef>>,
    pub capabilities: Arc<ContractCapabilities>,
    pub schemas: CompiledSchemas,
    pub operation_policy: CompiledOperationPolicy,
    pub behavioral: CompiledBehavioralMetadata,
    pub schema_fingerprint: u64,
    pub policy_fingerprint: u64,
    pub behavior_fingerprint: u64,
    pub provenance: DeclarationProvenance,
}

impl CompiledContract {
    pub fn compile(
        mut contract: InterfaceContract,
        provenance: DeclarationProvenance,
    ) -> Result<Self, ContractError> {
        contract.interface = crate::interface::canonical_interface_name_owned(contract.interface);
        if contract.interface.is_empty() {
            return Err(ContractError::Parse {
                interface: contract.interface,
                message: "interface identity cannot be empty".to_string(),
            });
        }
        if let Some(message) = contract_type_errors(&contract).into_iter().next() {
            return Err(ContractError::InvalidType {
                interface: contract.interface,
                message,
            });
        }
        validate_unique_symbols(&contract)?;

        let schemas = compile_schemas(&contract)?;
        let operation_policy = compile_operation_policy(&contract.capabilities);
        let behavioral = CompiledBehavioralMetadata {
            methods: Arc::new(
                contract
                    .methods
                    .iter()
                    .map(|method| {
                        (
                            method.name.clone(),
                            CompiledMethodBehavior {
                                coalesce: method.coalesce,
                                state_binding: method.state_binding.clone(),
                            },
                        )
                    })
                    .collect(),
            ),
        };
        let schema_fingerprint = fingerprint_schemas(&schemas);
        let policy_fingerprint = fingerprint_policy(&operation_policy);
        let behavior_fingerprint = fingerprint_behavior(&behavioral);
        let raw = Arc::new(contract.clone());

        Ok(Self {
            raw,
            interface: contract.interface.clone(),
            version: contract.version.clone(),
            state_fields: arc_slice(contract.state_fields.clone()),
            methods: arc_slice(contract.methods.clone()),
            events: arc_slice(contract.events.clone()),
            types: Arc::new(contract.types.clone()),
            capabilities: Arc::new(contract.capabilities.clone()),
            schemas,
            operation_policy,
            behavioral,
            schema_fingerprint,
            policy_fingerprint,
            behavior_fingerprint,
            provenance,
        })
    }

    pub fn to_interface_contract(&self) -> InterfaceContract {
        InterfaceContract {
            interface: self.interface.clone(),
            version: self.version.clone(),
            state_fields: self.state_fields.to_vec(),
            methods: self.methods.to_vec(),
            events: self.events.to_vec(),
            types: (*self.types).clone(),
            capabilities: (*self.capabilities).clone(),
        }
    }

    pub fn method(&self, name: &str) -> Option<&InterfaceMethod> {
        self.methods.iter().find(|method| method.name == name)
    }

    pub fn event(&self, name: &str) -> Option<&InterfaceEvent> {
        self.events.iter().find(|event| event.name == name)
    }

    pub fn type_definition(&self, name: &str) -> Option<&InterfaceTypeDef> {
        self.types.get(name)
    }
}

impl InterfaceContract {
    pub fn compile(
        &self,
        provenance: DeclarationProvenance,
    ) -> Result<CompiledContract, ContractError> {
        CompiledContract::compile(self.clone(), provenance)
    }
}

/// A documented core state field that providers must include in emitted payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractStateField {
    pub name: String,
    pub field_type: String,
    #[allow(dead_code)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceMethod {
    pub name: String,
    pub args: Vec<InterfaceArgument>,
    pub returns: Option<String>,
    /// When true, repeated invocations of this method on a backend's command
    /// queue are coalesced — only the most recent payload is executed and
    /// older queued instances are dropped. Right for idempotent setters
    /// (set_volume, set_muted) where intermediate values are stale; wrong for
    /// relative/accumulating commands (volume_up, increment).
    pub coalesce: bool,
    /// Optional reactive state binding. When the command is accepted, the
    /// shell writes the command value into the service's shared state and
    /// publishes it to every observer before the provider confirms it.
    pub state_binding: Option<StateBinding>,
}

/// Contract-declared binding between a command and shared service state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateBinding {
    /// Public service-state field controlled by the command.
    pub field: String,
    /// Command argument whose value is written into the shared state field.
    pub from_arg: Option<String>,
    /// Negate the current boolean state instead of copying an argument.
    pub toggle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceArgument {
    pub name: String,
    pub arg_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceEvent {
    pub name: String,
    /// Typed payload fields. Empty means the event carries no declared payload.
    pub payload: Vec<InterfaceArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceTypeDef {
    pub name: String,
    pub fields: Vec<InterfaceArgument>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractCapabilities {
    pub required: Vec<String>,
    pub optional: Vec<String>,
    /// Capability requirements for reading the state projection. When absent
    /// on a legacy contract, callers may use the compatibility policy.
    pub read: Vec<String>,
    /// Capability requirements keyed by declared event name.
    pub events: HashMap<String, Vec<String>>,
    /// Capability requirements keyed by declared method name.
    pub methods: HashMap<String, Vec<String>>,
}

impl ContractCapabilities {
    fn has_legacy_declaration(&self) -> bool {
        !self.required.is_empty() || !self.optional.is_empty()
    }

    /// Return the explicit read policy, or the legacy read subset when the
    /// contract predates operation policies. `None` means use compatibility
    /// behavior for an entirely undeclared test/legacy contract.
    pub fn read_policy(&self) -> Option<Vec<String>> {
        if !self.read.is_empty() {
            Some(self.read.clone())
        } else if self.has_legacy_declaration() {
            Some(
                self.required
                    .iter()
                    .filter(|capability| capability.ends_with(".read"))
                    .cloned()
                    .collect(),
            )
        } else {
            None
        }
    }

    pub fn event_policy(&self, event: &str) -> Option<Vec<String>> {
        self.events
            .get(event)
            .cloned()
            .or_else(|| self.read_policy())
    }

    /// Method policies fall back to declared control capabilities for legacy
    /// contracts. An explicit empty list remains a deliberate deny-all policy.
    pub fn method_policy(&self, method: &str) -> Option<Vec<String>> {
        if let Some(policy) = self.methods.get(method) {
            return Some(policy.clone());
        }
        if self.has_legacy_declaration() {
            Some(
                self.required
                    .iter()
                    .chain(self.optional.iter())
                    .filter(|capability| capability.ends_with(".control"))
                    .cloned()
                    .collect(),
            )
        } else {
            None
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("failed to parse interface contract for {interface}: {message}")]
    Parse { interface: String, message: String },

    #[error("invalid interface version '{value}' for {interface}")]
    InvalidVersion { interface: String, value: String },

    #[error("invalid type in contract for {interface}: {message}")]
    InvalidType { interface: String, message: String },

    #[error("invalid declaration in contract for {interface}: {message}")]
    InvalidDeclaration { interface: String, message: String },
}

/// A parsed type expression from the contract type grammar.
///
/// Grammar: `base`, `base[]`, `base?`, `base[]?` where `base` is a primitive
/// (`string`, `int`, `float`, `boolean`, `object`, `any`) or a named type
/// declared in the contract's `types` map (plus the builtin `Result`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeExpr {
    pub base: BaseType,
    pub array: bool,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseType {
    String,
    Int,
    Float,
    Boolean,
    Object,
    Any,
    Named(String),
}

/// Builtin named type for command results (`{ ok: boolean, error: string? }`).
pub const BUILTIN_RESULT_TYPE: &str = "Result";

impl TypeExpr {
    pub fn parse(expr: &str) -> Result<Self, String> {
        let mut rest = expr.trim();
        if rest.is_empty() {
            return Err("type expression cannot be empty".to_string());
        }
        let mut optional = false;
        if let Some(stripped) = rest.strip_suffix('?') {
            optional = true;
            rest = stripped.trim_end();
        }
        let mut array = false;
        if let Some(stripped) = rest.strip_suffix("[]") {
            array = true;
            rest = stripped.trim_end();
        }
        if rest.is_empty() {
            return Err(format!("type expression '{expr}' has no base type"));
        }
        let base = match rest {
            "string" => BaseType::String,
            "int" => BaseType::Int,
            "float" => BaseType::Float,
            "boolean" => BaseType::Boolean,
            "object" => BaseType::Object,
            "any" => BaseType::Any,
            named => {
                if !named
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
                    || !named.chars().all(|ch| ch.is_ascii_alphanumeric())
                {
                    return Err(format!(
                        "invalid type '{expr}': expected a primitive (string, int, float, boolean, object, any) or a PascalCase named type"
                    ));
                }
                BaseType::Named(named.to_string())
            }
        };
        Ok(Self {
            base,
            array,
            optional,
        })
    }

    /// Check a JSON value against this type expression. Named types match
    /// objects (structural field checks are the caller's concern).
    pub fn matches(&self, value: &JsonValue) -> bool {
        if value.is_null() {
            return self.optional;
        }
        if self.array {
            return value.is_array();
        }
        match &self.base {
            BaseType::String => value.is_string(),
            BaseType::Int => value.as_i64().is_some() || value.as_u64().is_some(),
            BaseType::Float => value.is_number(),
            BaseType::Boolean => value.is_boolean(),
            BaseType::Object => value.is_object(),
            BaseType::Any => true,
            BaseType::Named(_) => value.is_object(),
        }
    }

    /// Check a value against this expression and the contract's complete named
    /// type graph. Arrays validate every member and named objects validate
    /// every declared field recursively, including optional presence/null
    /// semantics. Cyclic graphs fail closed at the edge that would recurse.
    pub fn matches_with_types(
        &self,
        value: &JsonValue,
        types: &HashMap<String, InterfaceTypeDef>,
    ) -> bool {
        let mut visiting = HashSet::new();
        self.matches_inner(value, types, &mut visiting)
    }

    fn matches_inner(
        &self,
        value: &JsonValue,
        types: &HashMap<String, InterfaceTypeDef>,
        visiting: &mut HashSet<String>,
    ) -> bool {
        if value.is_null() {
            return self.optional;
        }
        if self.array {
            return value.as_array().is_some_and(|items| {
                items.iter().all(|item| {
                    let item_expr = Self {
                        base: self.base.clone(),
                        array: false,
                        optional: false,
                    };
                    item_expr.matches_inner(item, types, visiting)
                })
            });
        }
        match &self.base {
            BaseType::String => value.is_string(),
            BaseType::Int => value.as_i64().is_some() || value.as_u64().is_some(),
            BaseType::Float => value.is_number(),
            BaseType::Boolean => value.is_boolean(),
            BaseType::Object => value.is_object(),
            BaseType::Any => true,
            BaseType::Named(name) if name == BUILTIN_RESULT_TYPE => {
                let Some(object) = value.as_object() else {
                    return false;
                };
                object.get("ok").is_some_and(|ok| ok.is_boolean())
                    && object
                        .get("error")
                        .is_none_or(|error| error.is_string() || error.is_null())
            }
            BaseType::Named(name) => {
                if !visiting.insert(name.clone()) {
                    return false;
                }
                let matched = types.get(name).is_some_and(|definition| {
                    let Some(object) = value.as_object() else {
                        return false;
                    };
                    definition.fields.iter().all(|field| {
                        let Ok(field_type) = TypeExpr::parse(&field.arg_type) else {
                            return false;
                        };
                        match object.get(&field.name) {
                            Some(value) => field_type.matches_inner(value, types, visiting),
                            None => field_type.optional,
                        }
                    })
                });
                visiting.remove(name);
                matched
            }
        }
    }
}

fn arc_slice<T>(values: Vec<T>) -> Arc<[T]> {
    values.into()
}

fn validate_unique_symbols(contract: &InterfaceContract) -> Result<(), ContractError> {
    let duplicate = |kind: &str, name: &str| ContractError::InvalidDeclaration {
        interface: contract.interface.clone(),
        message: format!("duplicate {kind} declaration '{name}'"),
    };

    let mut symbols = HashSet::new();
    for (kind, names) in [
        (
            "state field",
            contract
                .state_fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "method",
            contract
                .methods
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "event",
            contract
                .events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
        ),
    ] {
        let mut local = HashSet::new();
        for name in names {
            if !local.insert(name) {
                return Err(duplicate(kind, name));
            }
            if !symbols.insert(name) {
                return Err(ContractError::InvalidDeclaration {
                    interface: contract.interface.clone(),
                    message: format!("symbol '{name}' overlaps another declaration"),
                });
            }
        }
    }

    let mut type_names = HashSet::new();
    for (name, definition) in &contract.types {
        if !type_names.insert(name.as_str()) {
            return Err(duplicate("named type", name));
        }
        let mut fields = HashSet::new();
        for field in &definition.fields {
            if !fields.insert(field.name.as_str()) {
                return Err(duplicate("type field", &format!("{}.{}", name, field.name)));
            }
        }
    }

    for method in &contract.methods {
        let mut args = HashSet::new();
        for arg in &method.args {
            if !args.insert(arg.name.as_str()) {
                return Err(duplicate(
                    "method argument",
                    &format!("{}.{}", method.name, arg.name),
                ));
            }
        }
    }
    for event in &contract.events {
        let mut fields = HashSet::new();
        for field in &event.payload {
            if !fields.insert(field.name.as_str()) {
                return Err(duplicate(
                    "event payload field",
                    &format!("{}.{}", event.name, field.name),
                ));
            }
        }
    }

    Ok(())
}

fn parse_compiled_type(
    interface: &str,
    context: String,
    value: &str,
) -> Result<TypeExpr, ContractError> {
    TypeExpr::parse(value).map_err(|message| ContractError::InvalidType {
        interface: interface.to_string(),
        message: format!("{context}: {message}"),
    })
}

fn compile_schemas(contract: &InterfaceContract) -> Result<CompiledSchemas, ContractError> {
    let state_fields = contract
        .state_fields
        .iter()
        .map(|field| {
            Ok(CompiledStateField {
                name: field.name.clone(),
                type_expr: parse_compiled_type(
                    &contract.interface,
                    format!("state field '{}'", field.name),
                    &field.field_type,
                )?,
                description: field.description.clone(),
            })
        })
        .collect::<Result<Vec<_>, ContractError>>()?;
    let methods = contract
        .methods
        .iter()
        .map(|method| {
            let args = method
                .args
                .iter()
                .map(|arg| {
                    Ok(CompiledField {
                        name: arg.name.clone(),
                        type_expr: parse_compiled_type(
                            &contract.interface,
                            format!("method '{}' arg '{}'", method.name, arg.name),
                            &arg.arg_type,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, ContractError>>()?;
            let returns = method
                .returns
                .as_deref()
                .map(|value| {
                    parse_compiled_type(
                        &contract.interface,
                        format!("method '{}' returns", method.name),
                        value,
                    )
                })
                .transpose()?;
            Ok(CompiledMethodSchema {
                name: method.name.clone(),
                args: arc_slice(args),
                returns,
                coalesce: method.coalesce,
                state_binding: method.state_binding.clone(),
            })
        })
        .collect::<Result<Vec<_>, ContractError>>()?;
    let events = contract
        .events
        .iter()
        .map(|event| {
            let payload = event
                .payload
                .iter()
                .map(|field| {
                    Ok(CompiledField {
                        name: field.name.clone(),
                        type_expr: parse_compiled_type(
                            &contract.interface,
                            format!("event '{}' payload field '{}'", event.name, field.name),
                            &field.arg_type,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, ContractError>>()?;
            Ok(CompiledEventSchema {
                name: event.name.clone(),
                payload: arc_slice(payload),
            })
        })
        .collect::<Result<Vec<_>, ContractError>>()?;
    let mut types = BTreeMap::new();
    for (name, definition) in &contract.types {
        let fields = definition
            .fields
            .iter()
            .map(|field| {
                Ok(CompiledField {
                    name: field.name.clone(),
                    type_expr: parse_compiled_type(
                        &contract.interface,
                        format!("type '{}' field '{}'", name, field.name),
                        &field.arg_type,
                    )?,
                })
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        types.insert(
            name.clone(),
            CompiledTypeSchema {
                name: name.clone(),
                fields: arc_slice(fields),
            },
        );
    }

    Ok(CompiledSchemas {
        state_fields: arc_slice(state_fields),
        methods: arc_slice(methods),
        events: arc_slice(events),
        types: Arc::new(types),
    })
}

fn normalize_capabilities(values: &[String]) -> Arc<[String]> {
    let mut values = values.to_vec();
    values.sort();
    values.dedup();
    arc_slice(values)
}

fn compile_operation_policy(capabilities: &ContractCapabilities) -> CompiledOperationPolicy {
    let read = capabilities
        .read_policy()
        .map(|values| normalize_capabilities(&values));
    let events = capabilities
        .events
        .iter()
        .map(|(name, values)| (name.clone(), normalize_capabilities(values)))
        .collect();
    let methods = capabilities
        .methods
        .iter()
        .map(|(name, values)| (name.clone(), normalize_capabilities(values)))
        .collect();
    CompiledOperationPolicy {
        read,
        events: Arc::new(events),
        methods: Arc::new(methods),
    }
}

#[derive(Default)]
struct FingerprintBuilder {
    bytes: Vec<u8>,
}

impl FingerprintBuilder {
    fn string(&mut self, value: &str) {
        self.bytes
            .extend_from_slice(&(value.len() as u64).to_le_bytes());
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn bool(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }

    fn type_expr(&mut self, value: &TypeExpr) {
        match &value.base {
            BaseType::String => self.bytes.push(1),
            BaseType::Int => self.bytes.push(2),
            BaseType::Float => self.bytes.push(3),
            BaseType::Boolean => self.bytes.push(4),
            BaseType::Object => self.bytes.push(5),
            BaseType::Any => self.bytes.push(6),
            BaseType::Named(name) => {
                self.bytes.push(7);
                self.string(name);
            }
        }
        self.bool(value.array);
        self.bool(value.optional);
    }

    fn finish(self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in self.bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}

fn fingerprint_schemas(schemas: &CompiledSchemas) -> u64 {
    let mut builder = FingerprintBuilder::default();
    for field in schemas.state_fields.iter() {
        builder.string(&field.name);
        builder.type_expr(&field.type_expr);
    }
    for method in schemas.methods.iter() {
        builder.string(&method.name);
        for arg in method.args.iter() {
            builder.string(&arg.name);
            builder.type_expr(&arg.type_expr);
        }
        if let Some(returns) = &method.returns {
            builder.bool(true);
            builder.type_expr(returns);
        } else {
            builder.bool(false);
        }
    }
    for event in schemas.events.iter() {
        builder.string(&event.name);
        for field in event.payload.iter() {
            builder.string(&field.name);
            builder.type_expr(&field.type_expr);
        }
    }
    for (name, definition) in schemas.types.iter() {
        builder.string(name);
        for field in definition.fields.iter() {
            builder.string(&field.name);
            builder.type_expr(&field.type_expr);
        }
    }
    builder.finish()
}

fn fingerprint_behavior(behavioral: &CompiledBehavioralMetadata) -> u64 {
    let mut builder = FingerprintBuilder::default();
    for (name, behavior) in behavioral.methods.iter() {
        builder.string(name);
        builder.bool(behavior.coalesce);
        if let Some(binding) = &behavior.state_binding {
            builder.bool(true);
            builder.string(&binding.field);
            if let Some(from_arg) = &binding.from_arg {
                builder.bool(true);
                builder.string(from_arg);
            } else {
                builder.bool(false);
            }
            builder.bool(binding.toggle);
        } else {
            builder.bool(false);
        }
    }
    builder.finish()
}

fn fingerprint_policy(policy: &CompiledOperationPolicy) -> u64 {
    let mut builder = FingerprintBuilder::default();
    if let Some(read) = &policy.read {
        builder.bool(true);
        for capability in read.iter() {
            builder.string(capability);
        }
    } else {
        builder.bool(false);
    }
    for (name, capabilities) in policy.events.iter() {
        builder.string(name);
        for capability in capabilities.iter() {
            builder.string(capability);
        }
    }
    for (name, capabilities) in policy.methods.iter() {
        builder.string(name);
        for capability in capabilities.iter() {
            builder.string(capability);
        }
    }
    builder.finish()
}

/// Parse and validate an inline or external contract JSON object into an
/// [`InterfaceContract`].
///
/// Every type expression in the contract is validated against the type
/// grammar, and named types must be declared in `types` (or be the builtin
/// `Result`).
pub fn parse_interface_contract(
    interface_name: &str,
    interface_version: &str,
    contract: &JsonValue,
) -> Result<InterfaceContract, ContractError> {
    let interface_name = crate::interface::canonical_interface_name(interface_name);
    if interface_name.is_empty() {
        return Err(ContractError::Parse {
            interface: interface_name,
            message: "interface identity cannot be empty".to_string(),
        });
    }
    let contract = normalize_keyed_contract_declarations(contract.clone()).map_err(|message| {
        ContractError::Parse {
            interface: interface_name.clone(),
            message,
        }
    })?;
    let parsed: ContractJson =
        serde_json::from_value(contract).map_err(|source| ContractError::Parse {
            interface: interface_name.clone(),
            message: source.to_string(),
        })?;

    let version =
        parse_contract_version(interface_version).ok_or_else(|| ContractError::InvalidVersion {
            interface: interface_name.clone(),
            value: interface_version.to_string(),
        })?;

    let contract = InterfaceContract {
        interface: interface_name.clone(),
        version,
        state_fields: parsed
            .state
            .into_iter()
            .map(|field| ContractStateField {
                name: field.name,
                field_type: field.field_type,
                description: field.description,
            })
            .collect(),
        methods: parsed
            .methods
            .into_iter()
            .map(|method| InterfaceMethod {
                name: method.name,
                args: method
                    .args
                    .into_iter()
                    .map(ContractFieldJson::into_argument)
                    .collect(),
                returns: method.returns,
                coalesce: method.coalesce,
                state_binding: method.state_binding.map(|value| StateBinding {
                    field: value.field,
                    from_arg: value.from_arg,
                    toggle: value.toggle,
                }),
            })
            .collect(),
        events: parsed
            .events
            .into_iter()
            .map(|event| InterfaceEvent {
                name: event.name,
                payload: event
                    .payload
                    .into_iter()
                    .map(ContractFieldJson::into_argument)
                    .collect(),
            })
            .collect(),
        types: parsed
            .types
            .into_iter()
            .map(|(name, def)| {
                let fields = def
                    .fields
                    .into_iter()
                    .map(ContractFieldJson::into_argument)
                    .collect();
                (name.clone(), InterfaceTypeDef { name, fields })
            })
            .collect(),
        capabilities: ContractCapabilities {
            required: parsed.capabilities.required,
            optional: parsed.capabilities.optional,
            read: parsed.capabilities.read,
            events: parsed.capabilities.events,
            methods: parsed.capabilities.methods,
        },
    };

    if let Some(message) = contract_type_errors(&contract).into_iter().next() {
        return Err(ContractError::InvalidType {
            interface: interface_name,
            message,
        });
    }

    Ok(contract)
}

/// Parse and compile a contract into the immutable artifact consumed by the
/// service catalog.
pub fn parse_compiled_contract(
    interface_name: &str,
    interface_version: &str,
    contract: &JsonValue,
) -> Result<CompiledContract, ContractError> {
    parse_compiled_contract_with_provenance(
        interface_name,
        interface_version,
        contract,
        DeclarationProvenance::unknown(),
    )
}

pub fn parse_compiled_contract_with_provenance(
    interface_name: &str,
    interface_version: &str,
    contract: &JsonValue,
    provenance: DeclarationProvenance,
) -> Result<CompiledContract, ContractError> {
    parse_interface_contract(interface_name, interface_version, contract)?.compile(provenance)
}

pub fn compile_interface_contract(
    contract: InterfaceContract,
    provenance: DeclarationProvenance,
) -> Result<CompiledContract, ContractError> {
    CompiledContract::compile(contract, provenance)
}

/// The compact external `contract.json` format keys state, methods, and
/// events by their public names. Normalize it to the established array form so
/// both authoring shapes compile through exactly the same validation path.
fn normalize_keyed_contract_declarations(mut contract: JsonValue) -> Result<JsonValue, String> {
    let object = contract
        .as_object_mut()
        .ok_or_else(|| "contract must be a JSON object".to_string())?;

    for section in ["state", "methods", "events"] {
        let Some(value) = object.get_mut(section) else {
            continue;
        };
        let Some(entries) = value.as_object() else {
            continue;
        };
        let mut normalized = Vec::with_capacity(entries.len());
        for (name, declaration) in entries {
            let Some(mut declaration) = declaration.as_object().cloned() else {
                return Err(format!("contract {section}.{name} must be a JSON object"));
            };
            match declaration.get("name") {
                Some(JsonValue::String(existing)) if existing == name => {}
                Some(JsonValue::String(existing)) => {
                    return Err(format!(
                        "contract {section}.{name} names itself '{existing}', which does not match its key"
                    ));
                }
                Some(_) => return Err(format!("contract {section}.{name}.name must be a string")),
                None => {
                    declaration.insert("name".into(), JsonValue::String(name.clone()));
                }
            }
            normalized.push(JsonValue::Object(declaration));
        }
        *value = JsonValue::Array(normalized);
    }
    Ok(contract)
}

fn check_type_expr(
    errors: &mut Vec<String>,
    types: &HashMap<String, InterfaceTypeDef>,
    context: String,
    expr: &str,
) {
    match TypeExpr::parse(expr) {
        Ok(parsed) => {
            if let BaseType::Named(name) = &parsed.base
                && name != BUILTIN_RESULT_TYPE
                && !types.contains_key(name)
            {
                errors.push(format!(
                    "{context}: named type '{name}' is not declared in types"
                ));
            }
        }
        Err(message) => errors.push(format!("{context}: {message}")),
    }
}

fn named_type_cycle(
    name: &str,
    types: &HashMap<String, InterfaceTypeDef>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) -> bool {
    if !visiting.insert(name.to_string()) {
        return true;
    }
    let cycle = types.get(name).is_some_and(|definition| {
        definition.fields.iter().any(|field| {
            let Ok(expr) = TypeExpr::parse(&field.arg_type) else {
                return false;
            };
            let BaseType::Named(next) = expr.base else {
                return false;
            };
            next != BUILTIN_RESULT_TYPE
                && types.contains_key(&next)
                && !visited.contains(&next)
                && named_type_cycle(&next, types, visiting, visited)
        })
    });
    visiting.remove(name);
    visited.insert(name.to_string());
    cycle
}

/// Collect every type-grammar violation in the contract. Empty means valid.
pub fn contract_type_errors(contract: &InterfaceContract) -> Vec<String> {
    let mut errors = Vec::new();
    let check = |errors: &mut Vec<String>, context: String, expr: &str| {
        check_type_expr(errors, &contract.types, context, expr)
    };

    for field in &contract.state_fields {
        check(
            &mut errors,
            format!("state field '{}'", field.name),
            &field.field_type,
        );
    }
    for method in &contract.methods {
        for arg in &method.args {
            check(
                &mut errors,
                format!("method '{}' arg '{}'", method.name, arg.name),
                &arg.arg_type,
            );
        }
        if let Some(returns) = &method.returns {
            check(
                &mut errors,
                format!("method '{}' returns", method.name),
                returns,
            );
        }
        if let Some(binding) = &method.state_binding {
            let bound_field = contract
                .state_fields
                .iter()
                .find(|field| field.name == binding.field);
            if bound_field.is_none() {
                errors.push(format!(
                    "method '{}' stateBinding field '{}' is not a declared state field",
                    method.name, binding.field
                ));
            }
            if binding.from_arg.is_some() == binding.toggle {
                errors.push(format!(
                    "method '{}' stateBinding must declare exactly one of fromArg or toggle=true",
                    method.name
                ));
            } else if let Some(from_arg) = &binding.from_arg {
                let source_arg = method.args.iter().find(|arg| &arg.name == from_arg);
                if source_arg.is_none() {
                    errors.push(format!(
                        "method '{}' stateBinding fromArg '{}' is not a declared argument",
                        method.name, from_arg
                    ));
                } else if let (Some(field), Some(arg)) = (bound_field, source_arg)
                    && field.field_type != arg.arg_type
                {
                    errors.push(format!(
                        "method '{}' stateBinding type mismatch: state field '{}' is '{}' but argument '{}' is '{}'",
                        method.name,
                        field.name,
                        field.field_type,
                        arg.name,
                        arg.arg_type
                    ));
                }
            } else if let Some(field) = bound_field
                && field.field_type != "boolean"
            {
                errors.push(format!(
                    "method '{}' toggle stateBinding field '{}' must be boolean",
                    method.name, field.name
                ));
            }
        }
    }
    for event in &contract.events {
        for field in &event.payload {
            check(
                &mut errors,
                format!("event '{}' payload field '{}'", event.name, field.name),
                &field.arg_type,
            );
        }
    }
    for event in contract.capabilities.events.keys() {
        if !contract
            .events
            .iter()
            .any(|declared| declared.name == *event)
        {
            errors.push(format!(
                "capabilities.events references undeclared event '{event}'"
            ));
        }
    }
    for method in contract.capabilities.methods.keys() {
        if !contract
            .methods
            .iter()
            .any(|declared| declared.name == *method)
        {
            errors.push(format!(
                "capabilities.methods references undeclared method '{method}'"
            ));
        }
    }
    for def in contract.types.values() {
        for field in &def.fields {
            check(
                &mut errors,
                format!("type '{}' field '{}'", def.name, field.name),
                &field.arg_type,
            );
        }
    }
    let mut visited = HashSet::new();
    for name in contract.types.keys() {
        if !visited.contains(name)
            && named_type_cycle(name, &contract.types, &mut HashSet::new(), &mut visited)
        {
            errors.push(format!("named type '{name}' contains a recursive cycle"));
        }
    }
    errors
}

pub fn parse_contract_version(value: &str) -> Option<Version> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    Version::parse(trimmed).ok().or_else(|| {
        let component_count = trimmed.split('.').count();
        let normalized = match component_count {
            1 => format!("{trimmed}.0.0"),
            2 => format!("{trimmed}.0"),
            _ => return None,
        };
        Version::parse(&normalized).ok()
    })
}

pub fn parse_version_req(value: &str) -> Option<VersionReq> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed
        .chars()
        .any(|ch| matches!(ch, '<' | '>' | '=' | '^' | '~' | ',' | '*'))
    {
        return VersionReq::parse(trimmed).ok();
    }

    parse_contract_version(trimmed)
        .and_then(|version| VersionReq::parse(&format!("={version}")).ok())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractJson {
    #[serde(default)]
    state: Vec<ContractStateFieldJson>,
    #[serde(default)]
    methods: Vec<ContractMethodJson>,
    #[serde(default)]
    events: Vec<ContractEventJson>,
    #[serde(default)]
    types: HashMap<String, ContractTypeJson>,
    #[serde(default)]
    capabilities: ContractCapabilitiesJson,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractStateFieldJson {
    name: String,
    #[serde(rename = "type")]
    field_type: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractMethodJson {
    name: String,
    #[serde(default)]
    args: Vec<ContractFieldJson>,
    #[serde(default)]
    returns: Option<String>,
    #[serde(default)]
    coalesce: bool,
    #[serde(default, rename = "stateBinding")]
    state_binding: Option<ContractStateBindingJson>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractStateBindingJson {
    field: String,
    #[serde(default, rename = "fromArg")]
    from_arg: Option<String>,
    #[serde(default)]
    toggle: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractEventJson {
    name: String,
    #[serde(default)]
    payload: Vec<ContractFieldJson>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractTypeJson {
    #[serde(default)]
    fields: Vec<ContractFieldJson>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractFieldJson {
    name: String,
    #[serde(rename = "type")]
    arg_type: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractCapabilitiesJson {
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    optional: Vec<String>,
    #[serde(default)]
    read: Vec<String>,
    #[serde(default)]
    events: HashMap<String, Vec<String>>,
    #[serde(default)]
    methods: HashMap<String, Vec<String>>,
}

impl ContractFieldJson {
    fn into_argument(self) -> InterfaceArgument {
        InterfaceArgument {
            name: self.name,
            arg_type: self.arg_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_semver_contract_version() {
        let version = parse_contract_version("1.0").unwrap();
        assert_eq!(version.to_string(), "1.0.0");
    }

    #[test]
    fn canonicalizes_short_interface_identity_when_compiling() {
        let contract = parse_interface_contract("audio", "1.0", &serde_json::json!({})).unwrap();
        assert_eq!(contract.interface, "mesh.audio");
    }

    #[test]
    fn compiles_immutable_schema_policy_and_provenance_artifact() {
        let contract = serde_json::json!({
            "state": [{ "name": "percent", "type": "float" }],
            "methods": [{
                "name": "set_percent",
                "args": [{ "name": "value", "type": "float" }],
                "returns": "Result",
                "coalesce": true,
                "stateBinding": { "field": "percent", "fromArg": "value" }
            }],
            "types": {
                "Reading": {
                    "fields": [{ "name": "value", "type": "float" }]
                }
            },
            "capabilities": {
                "read": ["service.audio.read"],
                "methods": { "set_percent": ["service.audio.control"] }
            }
        });
        let compiled = parse_compiled_contract_with_provenance(
            "audio",
            "1.0",
            &contract,
            DeclarationProvenance::new("@mesh/audio", "contract.json"),
        )
        .unwrap();

        assert_eq!(compiled.interface, "mesh.audio");
        assert_eq!(
            compiled.schemas.state_fields[0].type_expr,
            TypeExpr::parse("float").unwrap()
        );
        assert_eq!(compiled.schemas.types["Reading"].fields[0].name, "value");
        assert_eq!(
            compiled.operation_policy.read.as_ref().unwrap().as_ref(),
            &["service.audio.read".to_string()][..]
        );
        assert!(compiled.behavioral.methods["set_percent"].coalesce);
        assert_eq!(compiled.provenance.module.as_deref(), Some("@mesh/audio"));
        assert_ne!(compiled.schema_fingerprint, 0);
        assert_ne!(compiled.policy_fingerprint, 0);
        assert_ne!(compiled.behavior_fingerprint, 0);
    }

    #[test]
    fn compiled_fingerprints_ignore_capability_order_but_track_schema_changes() {
        let first = parse_compiled_contract(
            "mesh.audio",
            "1.0",
            &serde_json::json!({
                "state": [{ "name": "percent", "type": "float" }],
                "capabilities": { "read": ["b", "a"] }
            }),
        )
        .unwrap();
        let reordered = parse_compiled_contract(
            "mesh.audio",
            "1.0",
            &serde_json::json!({
                "state": [{ "name": "percent", "type": "float" }],
                "capabilities": { "read": ["a", "b"] }
            }),
        )
        .unwrap();
        let changed = parse_compiled_contract(
            "mesh.audio",
            "1.0",
            &serde_json::json!({
                "state": [{ "name": "percent", "type": "int" }],
                "capabilities": { "read": ["a", "b"] }
            }),
        )
        .unwrap();

        assert_eq!(first.policy_fingerprint, reordered.policy_fingerprint);
        assert_eq!(first.schema_fingerprint, reordered.schema_fingerprint);
        assert_ne!(first.schema_fingerprint, changed.schema_fingerprint);
    }

    #[test]
    fn compiled_contract_rejects_duplicate_and_overlapping_symbols() {
        let parsed = parse_interface_contract(
            "mesh.audio",
            "1.0",
            &serde_json::json!({
                "state": [
                    { "name": "percent", "type": "float" },
                    { "name": "percent", "type": "float" }
                ]
            }),
        )
        .unwrap();
        let error = parsed
            .compile(DeclarationProvenance::unknown())
            .unwrap_err();
        assert!(matches!(error, ContractError::InvalidDeclaration { .. }));
    }

    #[test]
    fn parses_exact_request_from_short_version() {
        let req = parse_version_req("1.0").unwrap();
        assert!(req.matches(&Version::parse("1.0.0").unwrap()));
        assert!(!req.matches(&Version::parse("1.1.0").unwrap()));
    }

    #[test]
    fn parses_contract_json_shape() {
        let contract = serde_json::json!({
            "methods": [
                { "name": "sensors", "returns": "Sensor[]" },
                {
                    "name": "read",
                    "args": [{ "name": "sensor_id", "type": "string" }],
                    "returns": "float"
                }
            ],
            "events": [
                {
                    "name": "TemperatureChanged",
                    "payload": [
                        { "name": "sensor_id", "type": "string" },
                        { "name": "celsius", "type": "float" }
                    ]
                }
            ],
            "types": {
                "Sensor": {
                    "fields": [
                        { "name": "id", "type": "string" },
                        { "name": "name", "type": "string" }
                    ]
                }
            },
            "capabilities": { "required": ["service.thermal.read"] }
        });

        let contract = parse_interface_contract("alice.thermal", "1.0", &contract).unwrap();

        assert_eq!(contract.interface, "alice.thermal");
        assert_eq!(contract.version.to_string(), "1.0.0");
        assert_eq!(contract.methods.len(), 2);
        assert_eq!(contract.methods[0].returns.as_deref(), Some("Sensor[]"));
        assert_eq!(contract.events[0].name, "TemperatureChanged");
        assert_eq!(contract.events[0].payload.len(), 2);
        assert_eq!(
            contract.capabilities.required,
            vec!["service.thermal.read".to_string()]
        );
        assert!(contract.types.contains_key("Sensor"));
    }

    #[test]
    fn parses_keyed_contract_json_shape() {
        let contract = serde_json::json!({
            "state": { "percent": { "type": "float" } },
            "methods": {
                "set_percent": {
                    "args": [{ "name": "value", "type": "float" }],
                    "stateBinding": { "field": "percent", "fromArg": "value" }
                }
            },
            "events": { "Changed": { "payload": [] } },
            "types": { "Device": { "fields": [] } }
        });
        let parsed = parse_interface_contract("mesh.audio", "1.0", &contract).unwrap();
        assert_eq!(parsed.state_fields[0].name, "percent");
        assert_eq!(parsed.methods[0].name, "set_percent");
        assert_eq!(parsed.events[0].name, "Changed");
        assert!(parsed.types.contains_key("Device"));
    }

    #[test]
    fn compiles_explicit_operation_capability_policy() {
        let contract = serde_json::json!({
            "state": [{ "name": "temperature", "type": "float" }],
            "methods": [{ "name": "calibrate", "returns": "Result" }],
            "events": [{ "name": "Changed" }],
            "capabilities": {
                "read": ["alice.thermal.observe"],
                "events": { "Changed": ["alice.thermal.subscribe"] },
                "methods": { "calibrate": ["alice.thermal.calibrate"] }
            }
        });
        let parsed = parse_interface_contract("alice.thermal", "1.0", &contract).unwrap();
        assert_eq!(
            parsed.capabilities.read_policy(),
            Some(vec!["alice.thermal.observe".to_string()])
        );
        assert_eq!(
            parsed.capabilities.event_policy("Changed"),
            Some(vec!["alice.thermal.subscribe".to_string()])
        );
        assert_eq!(
            parsed.capabilities.method_policy("calibrate"),
            Some(vec!["alice.thermal.calibrate".to_string()])
        );
    }

    #[test]
    fn rejects_operation_policy_for_unknown_declaration() {
        let contract = serde_json::json!({
            "capabilities": { "methods": { "calibrate": ["alice.thermal.calibrate"] } }
        });
        let err = parse_interface_contract("alice.thermal", "1.0", &contract).unwrap_err();
        assert!(err.to_string().contains("undeclared method 'calibrate'"));
    }

    #[test]
    fn parses_command_state_binding() {
        let contract = serde_json::json!({
            "state": [
                {
                    "name": "available",
                    "type": "boolean",
                    "description": "Whether the service is reachable"
                },
                { "name": "muted", "type": "boolean" }
            ],
            "methods": [
                {
                    "name": "set_muted",
                    "args": [
                        { "name": "device_id", "type": "string" },
                        { "name": "muted", "type": "boolean" }
                    ],
                    "returns": "Result",
                    "coalesce": true,
                    "stateBinding": { "field": "muted", "fromArg": "muted" }
                },
                {
                    "name": "toggle_mute",
                    "stateBinding": { "field": "muted", "toggle": true }
                }
            ]
        });

        let contract = parse_interface_contract("mesh.audio", "1.0", &contract).unwrap();

        assert_eq!(contract.state_fields.len(), 2);
        assert_eq!(contract.state_fields[0].name, "available");
        assert_eq!(
            contract.state_fields[0].description.as_deref(),
            Some("Whether the service is reachable")
        );
        let set_muted = &contract.methods[0];
        assert!(set_muted.coalesce);
        let binding = set_muted.state_binding.as_ref().unwrap();
        assert_eq!(binding.field, "muted");
        assert_eq!(binding.from_arg.as_deref(), Some("muted"));
        assert!(!binding.toggle);
        let toggle = contract.methods[1].state_binding.as_ref().unwrap();
        assert_eq!(toggle.field, "muted");
        assert_eq!(toggle.from_arg, None);
        assert!(toggle.toggle);
    }

    #[test]
    fn rejects_legacy_optimistic_annotation() {
        let contract = serde_json::json!({
            "state": [{ "name": "level", "type": "float" }],
            "methods": [{
                "name": "set",
                "args": [{ "name": "level", "type": "float" }],
                "optimistic": { "field": "level", "fromArg": "level" }
            }]
        });

        let err = parse_interface_contract("mesh.brightness", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::Parse { .. }));
        assert!(err.to_string().contains("unknown field `optimistic`"));
    }

    #[test]
    fn rejects_state_binding_type_mismatch() {
        let contract = serde_json::json!({
            "state": [{ "name": "level", "type": "float" }],
            "methods": [{
                "name": "set",
                "args": [{ "name": "level", "type": "string" }],
                "stateBinding": { "field": "level", "fromArg": "level" }
            }]
        });

        let err = parse_interface_contract("mesh.brightness", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
        assert!(err.to_string().contains("stateBinding type mismatch"));
    }

    #[test]
    fn rejects_toggle_binding_for_non_boolean_state() {
        let contract = serde_json::json!({
            "state": [{ "name": "level", "type": "float" }],
            "methods": [{
                "name": "toggle",
                "stateBinding": { "field": "level", "toggle": true }
            }]
        });

        let err = parse_interface_contract("mesh.brightness", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
        assert!(err.to_string().contains("must be boolean"));
    }

    #[test]
    fn rejects_undeclared_named_type() {
        let contract = serde_json::json!({
            "methods": [{ "name": "sensors", "returns": "Sensor[]" }]
        });
        let err = parse_interface_contract("alice.thermal", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
        assert!(err.to_string().contains("Sensor"));
    }

    #[test]
    fn rejects_invalid_type_expression() {
        let contract = serde_json::json!({
            "state": [{ "name": "percent", "type": "[float]" }]
        });
        let err = parse_interface_contract("mesh.audio", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
    }

    #[test]
    fn rejects_unknown_contract_keys() {
        let contract = serde_json::json!({ "state_fields": [] });
        let err = parse_interface_contract("mesh.audio", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::Parse { .. }));
    }

    #[test]
    fn type_expr_grammar_and_matching() {
        let expr = TypeExpr::parse("string").unwrap();
        assert!(expr.matches(&serde_json::json!("hi")));
        assert!(!expr.matches(&serde_json::json!(1)));
        assert!(!expr.matches(&serde_json::Value::Null));

        let expr = TypeExpr::parse("float?").unwrap();
        assert!(expr.matches(&serde_json::json!(1.5)));
        assert!(expr.matches(&serde_json::Value::Null));

        let expr = TypeExpr::parse("Sensor[]").unwrap();
        assert!(expr.array);
        assert!(expr.matches(&serde_json::json!([])));
        assert!(!expr.matches(&serde_json::json!({})));

        let expr = TypeExpr::parse("int").unwrap();
        assert!(expr.matches(&serde_json::json!(3)));
        assert!(!expr.matches(&serde_json::json!(3.5)));

        assert!(TypeExpr::parse("lowercaseNamed").is_err());
        assert!(TypeExpr::parse("").is_err());
        assert!(TypeExpr::parse("[Sensor]").is_err());
    }

    #[test]
    fn recursively_validates_named_arrays_optional_fields_and_results() {
        let contract = serde_json::json!({
            "types": {
                "Reading": {
                    "fields": [
                        { "name": "label", "type": "string" },
                        { "name": "unit", "type": "string?" }
                    ]
                },
                "Batch": {
                    "fields": [
                        { "name": "readings", "type": "Reading[]" }
                    ]
                }
            }
        });
        let parsed = parse_interface_contract("mesh.test", "1.0", &contract).unwrap();
        let batch = TypeExpr::parse("Batch").unwrap();
        let valid = serde_json::json!({
            "readings": [
                { "label": "temperature" },
                { "label": "humidity", "unit": null }
            ]
        });
        assert!(batch.matches_with_types(&valid, &parsed.types));
        assert!(!batch.matches_with_types(
            &serde_json::json!({ "readings": [{ "label": 7 }] }),
            &parsed.types
        ));
        assert!(!batch.matches_with_types(
            &serde_json::json!({ "readings": [{ "label": "temperature", "unit": 7 }] }),
            &parsed.types
        ));

        let result = TypeExpr::parse("Result").unwrap();
        assert!(result.matches_with_types(
            &serde_json::json!({ "ok": true, "value": { "count": 2 } }),
            &parsed.types
        ));
        assert!(!result.matches_with_types(&serde_json::json!({ "ok": "yes" }), &parsed.types));
    }

    #[test]
    fn rejects_recursive_named_type_cycles() {
        let contract = serde_json::json!({
            "types": {
                "Node": { "fields": [{ "name": "next", "type": "Node?" }] }
            }
        });
        let err = parse_interface_contract("mesh.test", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
        assert!(err.to_string().contains("recursive cycle"));
    }
}
