import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(new URL("../", import.meta.url)));
const pallasApp = join(root, "pallas-app");

mkdirSync(join(root, "data"), { recursive: true });

const backtestToml = join(root, "backtest.toml");
const backtestExample = join(root, "backtest.toml.example");
if (!existsSync(backtestToml) && existsSync(backtestExample)) {
  copyFileSync(backtestExample, backtestToml);
  console.log("Created backtest.toml from backtest.toml.example");
}

function run(label, command, args, cwd = root) {
  console.log(`\n> ${label}`);
  const result = spawnSync(command, args, { cwd, stdio: "inherit", shell: true });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

run("pnpm install (pallas-app)", "pnpm", ["install"], pallasApp);
run("cargo fetch (engine workspace)", "cargo", ["fetch"]);

console.log("\nSetup complete. Run `pnpm dev` from Backtesting-Engine to start the app.");
