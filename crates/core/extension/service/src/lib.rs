/// Interface plumbing for MESH's plugin runtime.
///
/// The source of truth is the interface contract plugin on disk plus the
/// backend plugin that provides it. This crate hosts the registry and contract
/// loader. All service interfaces are declared by plugins; there are no
/// hardcoded Rust trait adapters.
///
/// # Runtime model
///
/// ```text
///  interface contract plugin  +  backend plugin implementation
///                 |                         |
///                 +-----------+-------------+
///                             |
///                    InterfaceRegistry
///                             |
///                 frontend / scripting bindings
/// ```
///
/// - An **interface contract** defines methods, events, and capability names.
/// - A **backend plugin** provides an implementation of that contract.
/// - A **frontend plugin** consumes the interface through runtime bindings.
/// - The **registry** tracks discovered contracts and providers.
pub mod contract;
pub mod interface;
pub mod registry;

pub use contract::{
    ContractCapabilities, ContractError, InterfaceArgument, InterfaceContract, InterfaceEvent,
    InterfaceMethod, InterfaceTypeDef, load_interface_contract, parse_contract_version,
    parse_version_req,
};
pub use interface::{
    InterfaceCatalog, InterfaceProvider, InterfaceRegistry, InterfaceResolution,
    canonical_interface_name,
};
pub use registry::{ServiceEntry, ServiceError, ServiceRegistry};
