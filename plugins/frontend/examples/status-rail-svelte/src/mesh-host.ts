import {
  createMeshHostBridge,
  type HostEvent,
  type HostRequest,
  type JsonValue,
} from "../../../../../sdk/typescript/mesh-core-api/src/index";

type HostListener = (event: HostEvent) => void;

declare global {
  interface Window {
    __MESH_CORE__?: {
      postMessage(message: HostRequest): void;
      addEventListener(handler: HostListener): () => void;
    };
    meshHost?: ReturnType<typeof createMeshHostBridge>;
    __meshBindableStore__?: {
      get(id: string): JsonValue | undefined;
      set(id: string, value: JsonValue): void;
      subscribe(id: string, handler: (value: JsonValue) => void): () => void;
    };
  }
}

const bindableValues = new Map<string, JsonValue>();
const bindableListeners = new Map<string, Set<(value: JsonValue) => void>>();

function notifyBindable(id: string, value: JsonValue) {
  bindableValues.set(id, value);
  for (const handler of bindableListeners.get(id) ?? []) {
    handler(value);
  }
}

const bridge = createMeshHostBridge({
  postMessage(message) {
    window.__MESH_CORE__?.postMessage(message);
  },
  addEventListener(handler) {
    return window.__MESH_CORE__?.addEventListener(handler) ?? (() => {});
  },
});

bridge.onEvent((event) => {
  if (event.kind === "bindable_snapshot") {
    for (const [id, value] of Object.entries(event.values)) {
      notifyBindable(id, value);
    }
  }

  if (event.kind === "bindable_changed") {
    notifyBindable(event.id, event.value);
  }
});

window.meshHost = bridge;
window.__meshBindableStore__ = {
  get(id) {
    return bindableValues.get(id);
  },
  set(id, value) {
    notifyBindable(id, value);
  },
  subscribe(id, handler) {
    const handlers = bindableListeners.get(id) ?? new Set();
    handlers.add(handler);
    bindableListeners.set(id, handlers);
    return () => {
      handlers.delete(handler);
    };
  },
};
