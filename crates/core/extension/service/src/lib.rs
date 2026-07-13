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
/// - The **registry** tracks discovered contracts and providers.
pub mod contract;
pub mod interface;
pub mod registry;

pub use contract::{
    BaseType, ContractCapabilities, ContractError, ContractStateField, InterfaceArgument,
    InterfaceContract, InterfaceEvent, InterfaceMethod, InterfaceTypeDef, OptimisticUpdate,
    TypeExpr, contract_type_errors, parse_contract_version, parse_interface_contract,
    parse_version_req,
};
pub use interface::{
    InterfaceCatalog, InterfaceProvider, InterfaceRegistry, InterfaceResolution,
    canonical_interface_name, canonical_interface_name_cow, canonical_interface_name_owned,
    service_name_from_interface, service_name_from_interface_cow,
};
pub use registry::{ServiceEntry, ServiceError, ServiceRegistry};
