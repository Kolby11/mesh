/// Interface plumbing for MESH's module runtime.
///
/// The source of truth is the contract JSON declared in a module's
/// `module.json` (standalone interface module or inline in a backend module)
/// plus the backend module that provides it. This crate hosts the registry and
/// the contract parser. All service interfaces are declared by modules; there
/// are no hardcoded Rust trait adapters.
///
/// # Runtime model
///
/// ```text
///  interface contract module  +  backend module implementation
///                 |                         |
///                 +-----------+-------------+
///                             |
///                    InterfaceRegistry
///                             |
///                 frontend / scripting bindings
/// ```
///
/// - An **interface contract** defines methods, events, and capability names.
/// - A **backend module** provides an implementation of that contract.
/// - A **frontend module** consumes the interface through runtime bindings.
/// - The **interface catalog** tracks discovered contracts and providers.
pub mod compatibility;
pub mod contract;
pub mod generator;
pub mod interface;

pub use compatibility::{
    BidirectionalContractDiff, CompatibilityClass, CompatibilityClassification, ContractChange,
    ContractDiff, diff_contracts, diff_contracts_bidirectional,
};
pub use contract::{
    BaseType, CompiledBehavioralMetadata, CompiledContract, CompiledEventSchema,
    CompiledFeatureGroup, CompiledField, CompiledMethodBehavior, CompiledMethodSchema,
    CompiledOperationPolicy, CompiledSchemas, CompiledStateField, CompiledTypeSchema,
    ContractCapabilities, ContractError, ContractFeatureGroup, ContractStateField,
    DeclarationProvenance, FeatureNegotiation, InterfaceArgument, InterfaceContract,
    InterfaceEvent, InterfaceMethod, InterfaceTypeDef, StateBinding, TypeExpr,
    compile_interface_contract, contract_type_errors, negotiate_feature_groups,
    parse_compiled_contract, parse_compiled_contract_with_provenance, parse_contract_version,
    parse_interface_contract, parse_version_req,
};
pub use generator::{
    GeneratedContractArtifacts, generate_contract_artifacts, generate_contract_documentation,
    generate_luau_consumer_types, generate_luau_mock, generate_luau_provider_stub,
};
pub use interface::{
    InterfaceCatalog, InterfaceProvider, InterfaceRegistry, InterfaceResolution,
    ResolvedServiceCatalog, canonical_interface_name, canonical_interface_name_cow,
    canonical_interface_name_owned, service_name_from_interface, service_name_from_interface_cow,
};
