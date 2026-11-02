import { spawnSync, execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  copyFileSync,
  writeFileSync,
  readFileSync,
  readdirSync,
  unlinkSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { resolve, join } from "node:path";
const root = resolve(import.meta.dirname, "..");
process.chdir(root);
const env = { ...process.env };
if (process.platform === "win32") {
  const saved = JSON.parse(
    execFileSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        "@{CARGO_HOME=[Environment]::GetEnvironmentVariable('CARGO_HOME','Machine');RUSTUP_HOME=[Environment]::GetEnvironmentVariable('RUSTUP_HOME','Machine')} | ConvertTo-Json -Compress",
      ],
      { encoding: "utf8" },
    ),
  );
  if (!env.CARGO_HOME && saved.CARGO_HOME) env.CARGO_HOME = saved.CARGO_HOME;
  if (!env.RUSTUP_HOME && saved.RUSTUP_HOME)
    env.RUSTUP_HOME = saved.RUSTUP_HOME;
  if (env.CARGO_HOME) {
    env.Path = join(env.CARGO_HOME, "bin") + ";" + (env.Path || env.PATH || "");
    env.CARGO_TARGET_DIR ||= join(env.CARGO_HOME, "targets", "leafshield");
  }
}
const mode = process.argv[2] || "portable";
const version = JSON.parse(
  readFileSync(resolve("package.json"), "utf8"),
).version;
const run = (exe, args) => {
  const r = spawnSync(exe, args, { cwd: root, env, stdio: "inherit" });
  if (r.error) throw r.error;
  if (r.status !== 0) process.exit(r.status || 1);
};
const cargo =
  process.platform === "win32" && env.CARGO_HOME
    ? join(env.CARGO_HOME, "bin", "cargo.exe")
    : "cargo";
const cli = resolve("node_modules/@tauri-apps/cli/tauri.js");
if (mode === "test") run(cargo, ["test", "--workspace"]);
else if (mode === "check") {
  run(cargo, ["fmt", "--all", "--", "--check"]);
  run(cargo, [
    "clippy",
    "--workspace",
    "--all-targets",
    "--",
    "-D",
    "warnings",
  ]);
} else if (mode === "dev") run(process.execPath, [cli, "dev"]);
else if (mode === "smoke") {
  const exe = join(
    env.CARGO_TARGET_DIR || resolve("target"),
    "release",
    process.platform === "win32" ? "leafshield.exe" : "leafshield",
  );
  const report = resolve("artifacts/smoke.json");
  mkdirSync(resolve("artifacts"), { recursive: true });
  if (existsSync(report)) unlinkSync(report);
  run(exe, ["--smoke-test", report]);
  const result = JSON.parse(readFileSync(report, "utf8"));
  if (
    !result.ok ||
    !result.frontendReady ||
    result.monitors < 1 ||
    result.monitors !== result.restWindows
  )
    throw new Error(
      "Native smoke verification failed: " + JSON.stringify(result),
    );
  console.log(
    `Native UI and IPC passed; ${result.monitors} monitor(s), ${result.restWindows} rest window(s). Report: ${report}`,
  );
} else {
  const args =
    mode === "installer"
      ? [
          "build",
          "--bundles",
          process.platform === "win32"
            ? "nsis"
            : process.platform === "darwin"
              ? "dmg"
              : "deb,appimage",
        ]
      : ["build", "--no-bundle"];
  run(process.execPath, [cli, ...args]);
  mkdirSync(resolve("artifacts"), { recursive: true });
  if (mode !== "installer") {
    const name = process.platform === "win32" ? "leafshield.exe" : "leafshield";
    const source = join(
      env.CARGO_TARGET_DIR || resolve("target"),
      "release",
      name,
    );
    if (existsSync(source)) {
      mkdirSync(resolve("artifacts"), { recursive: true });
      const versioned =
        process.platform === "win32"
          ? `leafshield-${version}.exe`
          : `leafshield-${version}`;
      copyFileSync(source, resolve("artifacts", versioned));
      console.log("Portable executable: " + resolve("artifacts", versioned));
      try {
        copyFileSync(source, resolve("artifacts", name));
      } catch (error) {
        if (!["EBUSY", "EPERM", "EACCES"].includes(error.code)) throw error;
        console.log(
          "The running copy was preserved. Use " +
            versioned +
            " after closing the old app.",
        );
      }
    }
  } else {
    const bundle = join(
      env.CARGO_TARGET_DIR || resolve("target"),
      "release",
      "bundle",
    );
    for (const file of readdirSync(bundle, { recursive: true })) {
      if (/\.(exe|dmg|deb|AppImage)$/.test(file)) {
        const name = file.split(/[\\/]/).at(-1);
        copyFileSync(join(bundle, file), resolve("artifacts", name));
        console.log("Installer: " + resolve("artifacts", name));
      }
    }
  }
  const binaries = readdirSync(resolve("artifacts")).filter((name) =>
    /\.(exe|dmg|deb|AppImage)$/.test(name),
  );
  writeFileSync(
    resolve("artifacts/SHA256SUMS.txt"),
    binaries
      .map(
        (name) =>
          createHash("sha256")
            .update(readFileSync(resolve("artifacts", name)))
            .digest("hex") +
          "  " +
          name,
      )
      .join("\n") + "\n",
  );
}
