#!/usr/bin/env node
"use strict";

const net = require("net");
const readline = require("readline");
const fs = require("fs");
const path = require("path");

function usageAndExit() {
  const script = path.basename(process.argv[1] || "socket-json-pretty.js");
  console.error(`Usage: node ${script} <unix-socket-path>`);
  process.exit(1);
}

const socketPath = process.argv[2];
if (!socketPath) usageAndExit();

if (!fs.existsSync(socketPath)) {
  console.error(`Socket not found: ${socketPath}`);
  process.exit(1);
}

const socket = net.createConnection({ path: socketPath });
socket.setEncoding("utf8");

// Track pending commands to control when to re-show the prompt
let pendingCount = 0;
let nextId = 1;

function send(method, params) {
  const msg = { method, id: nextId++ };
  if (params !== undefined) msg.params = params;
  try {
    const line = JSON.stringify(msg);
    // Echo the outgoing JSON as a single line
    process.stdout.write("-> " + line + "\n");
    socket.write(line + "\n");
    pendingCount += 1;
  } catch (e) {
    console.error("Failed to send:", e?.message || String(e));
  }
}

// Read server output and print raw lines
const sockReader = readline.createInterface({ input: socket, crlfDelay: Infinity });
sockReader.on("line", (line) => {
  process.stdout.write("<- " + line + "\n");
  // Parse to see if this is a runCommandResult; if so, allow prompting
  try {
    const msg = JSON.parse(line);
    if (msg && ("result" in msg || "error" in msg)) {
      if (pendingCount > 0) pendingCount -= 1;
      if (pendingCount === 0) {
        repl.prompt();
      }
    }
  } catch (_) {
    // ignore parse errors; we still printed the raw line above
  }
});

// Simple REPL on stdin
const repl = readline.createInterface({ input: process.stdin, output: process.stdout, prompt: "$ " });

repl.on("line", (line) => {
  const raw = line.trim();
  if (raw.length === 0) {
    if (pendingCount === 0) repl.prompt();
    return;
  }

  if (raw.startsWith(":")) {
    const [cmd, ...rest] = raw.slice(1).split(/\s+/);
    const arg = rest.join(" ");
    switch (cmd) {
      case "env":
        send("getEnv");
        break;
      case "aliases":
        send("getAliases");
        break;
      case "getenv":
        if (!arg) {
          console.log("usage: :getenv NAME");
          if (pendingCount === 0) repl.prompt();
          return;
        }
        send("getEnvVar", { name: arg });
        break;
      case "getalias":
        if (!arg) {
          console.log("usage: :getalias NAME");
          if (pendingCount === 0) repl.prompt();
          return;
        }
        send("getAlias", { name: arg });
        break;
      case "help":
        console.log(
          [
            ":env                - get full env",
            ":aliases            - get all aliases",
            ":getenv NAME        - get env var",
            ":getalias NAME      - get alias value",
            ":quit               - exit",
          ].join("\n")
        );
        if (pendingCount === 0) repl.prompt();
        return;
      case "quit":
        repl.close();
        return;
      default:
        console.log(`unknown directive: :${cmd} (try :help)`);
        if (pendingCount === 0) repl.prompt();
        return;
    }
  } else {
    send("runCommand", { command: line });
  }
});

repl.on("SIGINT", () => {
  process.stdout.write("\n");
  repl.prompt();
});

repl.on("close", () => {
  socket.end();
  process.exit(0);
});

socket.on("connect", () => {
  // Only show prompt if no commands are pending
  if (pendingCount === 0) repl.prompt();
});

socket.on("error", (err) => {
  console.error("Socket error:", err?.message || String(err));
  process.exitCode = 2;
});

socket.on("end", () => {
  sockReader.close();
});

socket.on("close", () => {
  // Exit when the socket closes
  process.exit(process.exitCode || 0);
});
