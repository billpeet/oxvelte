import fs from "node:fs";
import path from "node:path";
export function compareProject(dir, app) {
  const read = (n) => JSON.parse(fs.readFileSync(path.join(dir, n)));

  const rel = (f) =>
    path.relative(app, f.replace(/^\\\\\?\\/, "")).replaceAll("\\", "/");
  const es = read("eslint.json");
  const ox = read("oxvelte.json");
  const fatal = es
    .filter((f) => f.messages.some((m) => m.fatal))
    .map((f) => ({
      file: rel(f.filePath),
      messages: f.messages.filter((m) => m.fatal),
    }));
  const blocked = new Set(fatal.map((f) => f.file));
  const e = es.flatMap((f) =>
    f.messages
      .filter((m) => m.ruleId?.startsWith("svelte/"))
      .map((m) => ({
        file: rel(f.filePath),
        rule: m.ruleId,
        line: m.line,
        col: m.column,
        endLine: m.endLine,
        endCol: m.endColumn,
        message: m.message,
        severity: m.severity,
      })),
  );
  const o = ox.map((m) => ({
    file: rel(m.file),
    rule: m.rule,
    line: m.line,
    col: m.column,
    endLine: m.endLine,
    endCol: m.endColumn,
    message: m.message,
    severity: m.severity === "error" ? 2 : 1,
  }));
  function compare(a, b, key) {
    const buckets = new Map();
    for (const item of b) {
      const k = key(item);
      if (!buckets.has(k)) buckets.set(k, []);
      buckets.get(k).push(item);
    }
    const matched = [],
      esOnly = [];
    for (const item of a) {
      const list = buckets.get(key(item));
      if (list?.length) matched.push({ eslint: item, oxvelte: list.shift() });
      else esOnly.push(item);
    }
    return {
      matched,
      eslintOnly: esOnly,
      oxvelteOnly: [...buckets.values()].flat(),
    };
  }
  const key = (d) => JSON.stringify([d.file, d.rule, d.line]);
  const exact = (d) =>
    JSON.stringify([
      d.file,
      d.rule,
      d.line,
      d.col,
      d.endLine,
      d.endCol,
      d.message,
      d.severity,
    ]);
  const comparableE = e.filter((d) => !blocked.has(d.file)),
    comparableO = o.filter((d) => !blocked.has(d.file));
  const lines = compare(comparableE, comparableO, key),
    details = compare(comparableE, comparableO, exact);
  const rules = [...new Set([...e, ...o].map((d) => d.rule))]
    .sort()
    .map((rule) => ({
      rule,
      eslint: e.filter((d) => d.rule === rule).length,
      oxvelte: o.filter((d) => d.rule === rule).length,
      comparableESLint: comparableE.filter((d) => d.rule === rule).length,
      comparableOxvelte: comparableO.filter((d) => d.rule === rule).length,
      lineMatches: lines.matched.filter((d) => d.eslint.rule === rule).length,
      exactMatches: details.matched.filter((d) => d.eslint.rule === rule)
        .length,
      eslintOnly: lines.eslintOnly.filter((d) => d.rule === rule).length,
      oxvelteOnly: lines.oxvelteOnly.filter((d) => d.rule === rule).length,
    }));
  const summary = {
    files: es.length,
    fatalFiles: fatal,
    raw: { eslint: e.length, oxvelte: o.length },
    comparable: {
      eslint: comparableE.length,
      oxvelte: comparableO.length,
      lineMatches: lines.matched.length,
      exactMatches: details.matched.length,
      eslintOnly: lines.eslintOnly.length,
      oxvelteOnly: lines.oxvelteOnly.length,
    },
    rules,
    excludedOxvelte: o.filter((d) => blocked.has(d.file)),
    eslintRun: read("eslint-run.json"),
    oxvelteRun: read("oxvelte-run.json"),
  };
  fs.writeFileSync(
    path.join(dir, "summary.json"),
    JSON.stringify(summary, null, 2) + "\n",
  );
  fs.writeFileSync(
    path.join(dir, "differences.json"),
    JSON.stringify(
      {
        eslintOnly: lines.eslintOnly,
        oxvelteOnly: lines.oxvelteOnly,
        diagnosticDifferences: lines.matched.filter(
          (p) => exact(p.eslint) !== exact(p.oxvelte),
        ),
        exactESLintOnly: details.eslintOnly,
        exactOxvelteOnly: details.oxvelteOnly,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    JSON.stringify(
      {
        files: summary.files,
        fatalFiles: summary.fatalFiles,
        raw: summary.raw,
        comparable: summary.comparable,
        rules: summary.rules,
      },
      null,
      2,
    ),
  );
  return summary;
}
