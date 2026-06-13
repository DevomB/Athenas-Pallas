import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const pallasApp = join(root, "pallas-app");

mkdirSync(join(root, "data"), { recursive: true });

function warnPrereq(label, command, args) {
  const result = spawnSync(command, args, {
    stdio: "ignore",
    shell: true,
    env: process.env,
  });
  if (result.status !== 0) {
    console.warn(`Warning: ${label} not found on PATH (${command} ${args.join(" ")})`);
  }
}

warnPrereq("Rust toolchain", "cargo", ["--version"]);
warnPrereq("Python (for external strategies)", "python", ["--version"]);

if (!existsSync(join(pallasApp, "node_modules"))) {
  console.log("First run: installing pnpm dependencies...");
  const install = spawnSync("pnpm", ["--dir", "pallas-app", "install"], {
    cwd: root,
    stdio: "inherit",
    shell: true,
    env: process.env,
  });
  if (install.status !== 0) {
    process.exit(install.status ?? 1);
  }
}

console.log("Starting Pallas (Vite UI + Tauri shell + Rust engine)...\n");

const child = spawn("pnpm", ["--dir", "pallas-app", "run", "tauri", "dev"], {
  cwd: root,
  stdio: "inherit",
  shell: true,
  env: process.env,
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});

for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => {
    if (!child.killed) {
      child.kill(signal);
    }
  });
}
