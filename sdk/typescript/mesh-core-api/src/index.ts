export type PluginRuntimeRole = "backend" | "frontend";

export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

export type BindableValueType =
  | "string"
  | "number"
  | "boolean"
  | "object"
  | "array"
  | "null";

export interface HostHello {
  plugin_id: string;
  role: PluginRuntimeRole;
  capabilities: string[];
  config: {
    dev_mode?: boolean;
    frontend_entry?: string;
    backend_entry?: string;
  };
}

export interface BindableValue {
  id: string;
  type: BindableValueType;
  mutable?: boolean;
  initial?: JsonValue;
}

export type HostRequest =
  | { kind: "register_bindable"; bindable: BindableValue }
  | { kind: "update_bindable"; id: string; value: JsonValue }
  | { kind: "subscribe_bindable"; id: string }
  | { kind: "unsubscribe_bindable"; id: string }
  | { kind: "invoke_core"; command: string; payload?: JsonValue }
  | { kind: "emit_event"; channel: string; payload?: JsonValue }
  | {
      kind: "register_frontend";
      component: {
        surface: string;
        framework: string;
        entry: string;
        props?: Record<string, JsonValue>;
        subscribes_to?: string[];
      };
    }
  | {
      kind: "register_backend";
      backend: {
        interface: string;
        entry: string;
        bindables?: string[];
      };
    };

export type HostEvent =
  | { kind: "ready"; hello: HostHello }
  | { kind: "bindable_snapshot"; values: Record<string, JsonValue> }
  | { kind: "bindable_changed"; id: string; value: JsonValue }
  | { kind: "core_event"; name: string; payload?: JsonValue }
  | { kind: "invoke_result"; request_id: string; payload?: JsonValue }
  | { kind: "error"; message: string };

export interface MeshHostBridge {
  send(message: HostRequest): void;
  onEvent(handler: (event: HostEvent) => void): () => void;
  registerBindable(bindable: BindableValue): void;
  updateBindable(id: string, value: JsonValue): void;
  subscribeBindable(id: string): void;
  emitEvent(channel: string, payload?: JsonValue): void;
  invokeCore(command: string, payload?: JsonValue): void;
}

export function createMeshHostBridge(
  transport: {
    postMessage(message: HostRequest): void;
    addEventListener(
      handler: (event: HostEvent) => void,
    ): () => void;
  },
): MeshHostBridge {
  return {
    send(message) {
      transport.postMessage(message);
    },
    onEvent(handler) {
      return transport.addEventListener(handler);
    },
    registerBindable(bindable) {
      transport.postMessage({ kind: "register_bindable", bindable });
    },
    updateBindable(id, value) {
      transport.postMessage({ kind: "update_bindable", id, value });
    },
    subscribeBindable(id) {
      transport.postMessage({ kind: "subscribe_bindable", id });
    },
    emitEvent(channel, payload) {
      transport.postMessage({ kind: "emit_event", channel, payload });
    },
    invokeCore(command, payload) {
      transport.postMessage({ kind: "invoke_core", command, payload });
    },
  };
}
