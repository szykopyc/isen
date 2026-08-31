const vscode = require("vscode");
const { spawn } = require("child_process");

class IsenClient {
  constructor(executable) {
    this.process = spawn(executable, ["lsp"]);
    this.nextId = 1;
    this.pending = new Map();
    this.buffer = Buffer.alloc(0);
    this.process.stdout.on("data", chunk => this.read(chunk));
    this.process.on("error", error => vscode.window.showWarningMessage(`Isen LSP: ${error.message}`));
    this.request("initialize", {
      processId: process.pid,
      capabilities: {},
      rootUri: vscode.workspace.workspaceFolders?.[0]?.uri.toString() ?? null
    }).then(() => this.notify("initialized", {}));
  }

  send(message) {
    const body = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...message }));
    this.process.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.process.stdin.write(body);
  }

  notify(method, params) {
    this.send({ method, params });
  }

  request(method, params) {
    const id = this.nextId++;
    this.send({ id, method, params });
    return new Promise(resolve => this.pending.set(id, resolve));
  }

  read(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    while (true) {
      const split = this.buffer.indexOf("\r\n\r\n");
      if (split < 0) return;
      const header = this.buffer.subarray(0, split).toString();
      const match = /Content-Length:\s*(\d+)/i.exec(header);
      if (!match) return;
      const length = Number(match[1]);
      const start = split + 4;
      if (this.buffer.length < start + length) return;
      const message = JSON.parse(this.buffer.subarray(start, start + length).toString());
      this.buffer = this.buffer.subarray(start + length);
      const resolve = this.pending.get(message.id);
      if (resolve) {
        this.pending.delete(message.id);
        resolve(message.result);
      }
    }
  }

  open(document) {
    this.notify("textDocument/didOpen", {
      textDocument: {
        uri: document.uri.toString(),
        languageId: "isen",
        version: document.version,
        text: document.getText()
      }
    });
  }

  change(document) {
    this.notify("textDocument/didChange", {
      textDocument: { uri: document.uri.toString(), version: document.version },
      contentChanges: [{ text: document.getText() }]
    });
  }

  stop() {
    this.request("shutdown", null).finally(() => {
      this.notify("exit", null);
      this.process.kill();
    });
  }
}

let client;

function activate(context) {
  const executable = vscode.workspace.getConfiguration("isen").get("executable", "isen");
  client = new IsenClient(executable);
  const isIsen = document => document.languageId === "isen";
  vscode.workspace.textDocuments.filter(isIsen).forEach(document => client.open(document));
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(document => isIsen(document) && client.open(document)),
    vscode.workspace.onDidChangeTextDocument(event => isIsen(event.document) && client.change(event.document)),
    vscode.languages.registerHoverProvider("isen", {
      async provideHover(document, position) {
        const result = await client.request("textDocument/hover", {
          textDocument: { uri: document.uri.toString() },
          position
        });
        return result ? new vscode.Hover(new vscode.MarkdownString(result.contents.value)) : null;
      }
    })
  );
}

function deactivate() {
  client?.stop();
}

module.exports = { activate, deactivate };
