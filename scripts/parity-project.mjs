import fs from "node:fs";
import path from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
// Compare a project's own ESLint Svelte rules with the current release binary.
// Usage: node scripts/parity-project.mjs <project> [--eslint-only|--reuse-eslint]
// The comparison uses diagnostic multiplicity, source ranges, messages and severity.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
if (!process.argv[2])
  throw Error(
    "Usage: node scripts/parity-project.mjs <project> [--eslint-only|--reuse-eslint]",
  );
const app = path.resolve(process.argv[2]);
const mode = process.argv[3];
if (mode && !["--eslint-only", "--reuse-eslint"].includes(mode))
  throw Error("Unknown mode: " + mode);
const out = path.join(root, "reports/parity-project", path.basename(app));
const binary =
  process.env.OXVELTE_BINARY ||
  path.join(
    root,
    "target/release/" +
      (process.platform === "win32" ? "oxvelte.exe" : "oxvelte"),
  );
fs.mkdirSync(out, { recursive: true });
const write = (name, data) =>
  fs.writeFileSync(path.join(out, name), JSON.stringify(data, null, 2) + "\n");
process.chdir(app);
const { ESLint } = await import(
  pathToFileURL(path.join(app, "node_modules/eslint/lib/api.js"))
);
const eslint = new ESLint({ cwd: app, errorOnUnmatchedPattern: false });
if (mode !== "--reuse-eslint") {
  const start = performance.now();
  const results = await eslint.lintFiles([
    "**/*.svelte",
    "**/*.svelte.js",
    "**/*.svelte.ts",
  ]);
  write("eslint.json", results);
  const groups = new Map();
  const allRules = new Set();
  for (const file of fs.readdirSync(path.join(root, "src/linter/rules"))) {
    if (!file.endsWith(".rs")) continue;
    const text = fs.readFileSync(
      path.join(root, "src/linter/rules", file),
      "utf8",
    );
    for (const match of text.matchAll(/"(svelte\/[a-z0-9-]+)"/g))
      allRules.add(match[1]);
  }
  const configs = [];
  for (const result of results) {
    const cfg = await eslint.calculateConfigForFile(result.filePath);
    if (!cfg) continue;
    const rules = Object.fromEntries(
      [...allRules].sort().map((r) => [r, "off"]),
    );
    for (const [r, v] of Object.entries(cfg.rules))
      if (r.startsWith("svelte/")) rules[r] = v;
    const svelteConfig = cfg.languageOptions?.parserOptions?.svelteConfig;
    const config = {
      rules,
      settings: {
        ...cfg.settings,
        compiler: {
          svelteConfig: {
            compilerOptions: svelteConfig?.compilerOptions || {},
          },
        },
      },
    };
    const key = JSON.stringify(config);
    if (!groups.has(key)) groups.set(key, { config, files: [] });
    groups.get(key).files.push(result.filePath);
    configs.push({
      file: result.filePath,
      rules: cfg.rules,
      settings: cfg.settings,
    });
  }
  write("configs.json", configs);
  write("groups.json", [...groups.values()]);
  write("eslint-run.json", {
    seconds: (performance.now() - start) / 1000,
    files: results.length,
    groups: groups.size,
  });
  console.log(
    "ESLint complete",
    results.length,
    "files",
    groups.size,
    "config groups",
  );
}
if (mode === "--eslint-only") process.exit(0);
const groups = JSON.parse(fs.readFileSync(path.join(out, "groups.json")));
const results = [];
const runs = [];
const start = performance.now();
for (const [i, group] of groups.entries()) {
  for (const file of fs.readdirSync(path.join(root, "src/linter/rules"))) {
    if (!file.endsWith(".rs")) continue;
    const text = fs.readFileSync(
      path.join(root, "src/linter/rules", file),
      "utf8",
    );
    for (const match of text.matchAll(
      /fn name\(&self\) -> &'static str\s*\{\s*"([^"]+)"/g,
    ))
      if (!match[1].startsWith("svelte/")) group.config.rules[match[1]] = "off";
  }
  const cfg = await eslint.calculateConfigForFile(group.files[0]);
  const compilerOptions =
    cfg.languageOptions?.parserOptions?.svelteConfig?.compilerOptions || {};
  group.config.settings = {
    ...group.config.settings,
    compiler: { svelteConfig: { compilerOptions } },
  };
  const configPath = path.join(out, `oxvelte-config-${i}.json`);
  fs.writeFileSync(configPath, JSON.stringify(group.config));
  for (let n = 0; n < group.files.length; n += 35) {
    const run = spawnSync(
      binary,
      [
        "lint",
        "--all-rules",
        "--config",
        configPath,
        "--json",
        ...group.files.slice(n, n + 35),
      ],
      {
        cwd: app,
        encoding: "utf8",
        maxBuffer: 64 * 1024 * 1024,
        env: { ...process.env, OXVELTE_COMPILER_RUNTIME: "" },
      },
    );
    runs.push({
      group: i,
      batch: n,
      status: run.status,
      stderr: run.stderr,
      error: run.error?.message,
    });
    write("oxvelte-runs.json", runs);
    if (![0, 1].includes(run.status)) throw Error(JSON.stringify(runs.at(-1)));
    results.push(...JSON.parse(run.stdout));
    write("oxvelte.json", results);
    console.log(
      "Oxvelte",
      i,
      n + Math.min(35, group.files.length - n),
      "/",
      group.files.length,
    );
  }
}
write("oxvelte-run.json", {
  seconds: (performance.now() - start) / 1000,
  files: groups.reduce((n, g) => n + g.files.length, 0),
});

const { compareProject } = await import("./compare-project-parity.mjs");
compareProject(out, app);
