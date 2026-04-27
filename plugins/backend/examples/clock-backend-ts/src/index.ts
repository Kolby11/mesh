import {
  createMeshHostBridge,
  type HostEvent,
} from "../../../../../sdk/typescript/mesh-core-api/src/index.ts";

const listeners = new Set<(event: HostEvent) => void>();
let transportClosed = false;
let tickHandle: ReturnType<typeof setInterval> | null = null;

function shutdown() {
  if (transportClosed) {
    return;
  }

  transportClosed = true;
  if (tickHandle) {
    clearInterval(tickHandle);
    tickHandle = null;
  }
}

function closeForHostDisconnect(error?: NodeJS.ErrnoException | null) {
  if (!error || error.code === "EPIPE" || error.code === "ERR_STREAM_DESTROYED") {
    shutdown();
    process.exit(0);
  }
}

const bridge = createMeshHostBridge({
  postMessage(message) {
    if (transportClosed) {
      return;
    }

    const encoded = JSON.stringify(message);

    try {
      process.stdout.write(`${encoded}\n`, (error) => {
        closeForHostDisconnect(error);
      });
    } catch (error) {
      closeForHostDisconnect(error as NodeJS.ErrnoException);
      throw error;
    }
  },
  addEventListener(handler) {
    listeners.add(handler);
    return () => listeners.delete(handler);
  },
});

process.stdin.setEncoding("utf8");
process.stdin.on("end", shutdown);
process.stdin.on("close", shutdown);
process.stdout.on("close", shutdown);
process.stdout.on("error", (error) => {
  closeForHostDisconnect(error as NodeJS.ErrnoException);
});

let buffer = "";
process.stdin.on("data", (chunk) => {
  if (transportClosed) {
    return;
  }

  buffer += chunk;

  while (true) {
    const newline = buffer.indexOf("\n");
    if (newline === -1) {
      break;
    }

    const line = buffer.slice(0, newline).trim();
    buffer = buffer.slice(newline + 1);
    if (!line) {
      continue;
    }

    const event = JSON.parse(line) as HostEvent;
    for (const listener of listeners) {
      listener(event);
    }
  }
});

bridge.registerBindable({
  id: "time.now",
  type: "string",
  initial: new Date().toISOString(),
});

bridge.send({
  kind: "register_backend",
  backend: {
    interface: "mesh.time",
    entry: "src/index.ts",
    bindables: ["time.now"],
  },
});

tickHandle = setInterval(() => {
  if (transportClosed) {
    return;
  }

  bridge.updateBindable("time.now", new Date().toISOString());
}, 1000);
