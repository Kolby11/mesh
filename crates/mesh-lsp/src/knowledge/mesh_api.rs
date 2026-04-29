pub struct MeshApiEntry {
    /// Path after "mesh." (e.g. "state.set").
    pub path: &'static str,
    pub signature: &'static str,
    pub description: &'static str,
    /// Whether this API is only available in backend scripts.
    pub backend_only: bool,
}

pub static MESH_API_ENTRIES: &[MeshApiEntry] = &[
    // State
    MeshApiEntry {
        path: "state.set",
        signature: "mesh.state.set(key: string, value: any)",
        description: "Declare a reactive state variable. Sets a Lua global with `key` and registers it for template binding.",
        backend_only: false,
    },
    MeshApiEntry {
        path: "state.get",
        signature: "mesh.state.get(key: string) -> any",
        description: "Read the current value of a state variable.",
        backend_only: false,
    },
    // Service (frontend)
    MeshApiEntry {
        path: "service.bind",
        signature: "mesh.service.bind(service_field: string, local_name: string)",
        description: "Bind a service field (e.g. \"audio.volume\") to a Lua global. The global is updated whenever the service emits.",
        backend_only: false,
    },
    MeshApiEntry {
        path: "service.on",
        signature: "mesh.service.on(service_name: string, handler: string)",
        description: "Register a Lua function to call whenever a service emits an update.",
        backend_only: false,
    },
    // Service (backend)
    MeshApiEntry {
        path: "service.emit",
        signature: "mesh.service.emit(data: table)",
        description: "Emit service state to all listening frontend plugins.",
        backend_only: true,
    },
    MeshApiEntry {
        path: "service.emit_json",
        signature: "mesh.service.emit_json(json_string: string?)",
        description: "Parse a JSON string and emit it as service state. If nil, emits the last received command payload.",
        backend_only: true,
    },
    MeshApiEntry {
        path: "service.emit_unavailable",
        signature: "mesh.service.emit_unavailable()",
        description: "Emit an unavailable signal — the service is not reachable.",
        backend_only: true,
    },
    MeshApiEntry {
        path: "service.set_poll_interval",
        signature: "mesh.service.set_poll_interval(ms: number)",
        description: "Set how often the backend plugin's poll cycle runs (in milliseconds).",
        backend_only: true,
    },
    MeshApiEntry {
        path: "service.payload",
        signature: "mesh.service.payload() -> table",
        description: "Get the current command payload sent by the shell or a frontend plugin.",
        backend_only: true,
    },
    MeshApiEntry {
        path: "service.has_capability",
        signature: "mesh.service.has_capability(name: string) -> boolean",
        description: "Check whether the plugin was granted a specific capability at startup.",
        backend_only: true,
    },
    // Interfaces
    MeshApiEntry {
        path: "interfaces.get",
        signature: "mesh.interfaces.get(name: string, version: string?) -> proxy",
        description: "Obtain a proxy for a service interface (e.g. \"mesh.audio\"). Returns nil if not available.",
        backend_only: false,
    },
    // Events
    MeshApiEntry {
        path: "events.subscribe",
        signature: "mesh.events.subscribe(event_name: string, handler: string)",
        description: "Subscribe to a named event on the shell event bus.",
        backend_only: false,
    },
    MeshApiEntry {
        path: "events.publish",
        signature: "mesh.events.publish(event_name: string, data: table?)",
        description: "Publish an event to the shell event bus.",
        backend_only: false,
    },
    // Theme
    MeshApiEntry {
        path: "theme.token",
        signature: "mesh.theme.token(name: string) -> string",
        description: "Resolve a theme token to its current value (e.g. a color hex string).",
        backend_only: false,
    },
    // Locale
    MeshApiEntry {
        path: "locale.translate",
        signature: "mesh.locale.translate(key: string, params: table?) -> string",
        description: "Translate a string key using the active locale.",
        backend_only: false,
    },
    // UI
    MeshApiEntry {
        path: "ui.request_redraw",
        signature: "mesh.ui.request_redraw()",
        description: "Request an immediate repaint of the component.",
        backend_only: false,
    },
    // Exec (backend)
    MeshApiEntry {
        path: "exec",
        signature: "mesh.exec(program: string, args: {string}?) -> {success, stdout, stderr, code}",
        description: "Run an external program and return its output.",
        backend_only: true,
    },
    MeshApiEntry {
        path: "exec_shell",
        signature: "mesh.exec_shell(command: string) -> {success, stdout, stderr, code}",
        description: "Run a shell command via `sh -lc` and return its output.",
        backend_only: true,
    },
    // Log
    MeshApiEntry {
        path: "log.info",
        signature: "mesh.log.info(message: string)",
        description: "Log an info message to the shell diagnostics.",
        backend_only: false,
    },
    MeshApiEntry {
        path: "log.warn",
        signature: "mesh.log.warn(message: string)",
        description: "Log a warning to the shell diagnostics.",
        backend_only: false,
    },
    MeshApiEntry {
        path: "log.error",
        signature: "mesh.log.error(message: string)",
        description: "Log an error to the shell diagnostics.",
        backend_only: false,
    },
];
