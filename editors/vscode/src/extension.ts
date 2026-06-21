import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const serverPath = resolveServerPath();
  if (!serverPath) {
    vscode.window.showErrorMessage(
      "MESH: could not find the mesh-tools-lsp binary. Build it with " +
        "`cargo build --release -p mesh-tools-lsp`, or set `mesh.lsp.serverPath`.",
    );
    return;
  }

  const serverOptions: ServerOptions = {
    run: { command: serverPath, transport: TransportKind.stdio },
    debug: { command: serverPath, transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    // .mesh components plus the two manifest filenames (handled as JSON).
    documentSelector: [
      { scheme: "file", language: "mesh" },
      { scheme: "file", language: "json", pattern: "**/module.json" },
      { scheme: "file", language: "json", pattern: "**/package.json" },
      { scheme: "file", language: "jsonc", pattern: "**/module.json" },
      { scheme: "file", language: "jsonc", pattern: "**/package.json" },
    ],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher(
        "**/{module.json,package.json,*.mesh}",
      ),
    },
  };

  client = new LanguageClient(
    "mesh",
    "MESH Language Server",
    serverOptions,
    clientOptions,
  );

  await client.start();
  context.subscriptions.push({ dispose: () => client?.stop() });
}

export async function deactivate(): Promise<void> {
  await client?.stop();
  client = undefined;
}

/** Locate the language server binary from settings, the workspace, or PATH. */
function resolveServerPath(): string | undefined {
  const configured = vscode.workspace
    .getConfiguration("mesh")
    .get<string>("lsp.serverPath");
  if (configured && configured.trim().length > 0) {
    return fs.existsSync(configured) ? configured : undefined;
  }

  const binName =
    process.platform === "win32" ? "mesh-tools-lsp.exe" : "mesh-tools-lsp";

  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const root = folder.uri.fsPath;
    const candidates = [
      path.join(root, "target", "release", binName),
      path.join(root, "target", "debug", binName),
    ];
    for (const candidate of candidates) {
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }
  }

  // Fall back to PATH; the spawn fails clearly if it is not there.
  return binName;
}
