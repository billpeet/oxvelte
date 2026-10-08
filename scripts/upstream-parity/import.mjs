// Mirrors discovery/config precedence in the pinned tests/utils/utils.ts.
// This exports raw cases, without generating or rewriting upstream expectations.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse as parseYaml } from 'yaml';
import semver from 'semver';
import ts from 'typescript';
import { createRequire } from 'node:module';
import { builtinRules } from 'eslint/use-at-your-own-risk';

export const revision = '18339c886320151148568063c5801bf69cb51027';
export const environment = {
  svelte: '5.49.2', eslint: '10.9.1', typescript: '6.0.3',
  '@typescript-eslint/parser': '8.70.0', 'svelte-eslint-parser': '1.8.1',
};
const require = createRequire(import.meta.url);
if (require('eslint/package.json').version !== environment.eslint) throw new Error('Installed ESLint does not match the declared parity environment');
if (ts.version !== environment.typescript) throw new Error('Installed TypeScript does not match the declared parity environment');
const repository = 'https://github.com/sveltejs/eslint-plugin-svelte';
const packagePrefix = 'packages/eslint-plugin-svelte/';
const fixturesPrefix = `${packagePrefix}tests/fixtures/rules/`;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const corpusRoot = path.join(repoRoot, 'fixtures/upstream/eslint-plugin-svelte');
export const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
export const suffix = (name) => /\.svelte\.[jt]s$/u.test(name) ? name.slice(-10) : path.extname(name);
export const isInput = (name) => {
  const base = name.slice(0, name.length - suffix(name).length);
  return !name.startsWith('_') && (base.endsWith('input') || base.startsWith('+'));
};
export const companion = (input, replacement) => input.replace(/(input|\+.+)(?:\.[a-z]+)+$/u, replacement);
export function configCandidates(input) {
  return ['json', 'js', 'cjs'].flatMap((ext) => [
    companion(input, `config.${ext}`), path.posix.join(path.posix.dirname(input), `_config.${ext}`),
  ]);
}
export function ineligible(requirements, versions = environment) {
  return Object.entries(requirements).filter(([name, range]) => {
    if (name === 'FIXME') return false;
    if (!versions[name]) throw new Error(`No declared version for requirement ${name}`);
    if (!semver.validRange(range)) throw new Error(`Invalid version range: ${name} ${range}`);
    return !semver.satisfies(versions[name], range);
  }).map(([name, range]) => `${name} ${versions[name]} does not satisfy ${range}`);
}

// Inspect the rule's actual metadata and statically provable runtime guards.
// Never execute imported rule modules or infer capabilities from fixture names.
export function ruleMetadata(source, versions = environment, coreRules = builtinRules) {
  const file = ts.createSourceFile('rule.ts', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const declarations = new Map();
  const coreLoaders = new Set();
  const compilerVersions = new Set();
  const semverNames = new Set();
  for (const statement of file.statements) {
    if (ts.isImportDeclaration(statement) && ts.isStringLiteral(statement.moduleSpecifier)) {
      const module = statement.moduleSpecifier.text;
      const clause = statement.importClause;
      if (module === 'semver' && clause?.name) semverNames.add(clause.name.text);
      if (clause?.namedBindings && ts.isNamedImports(clause.namedBindings)) {
        for (const element of clause.namedBindings.elements) {
          const imported = element.propertyName?.text ?? element.name.text;
          if (module.endsWith('/eslint-core.js') && imported === 'getCoreRule') coreLoaders.add(element.name.text);
          if (module === 'svelte/compiler' && imported === 'VERSION') compilerVersions.add(element.name.text);
        }
      }
    }
    if (ts.isVariableStatement(statement) && (statement.declarationList.flags & ts.NodeFlags.Const)) for (const declaration of statement.declarationList.declarations) {
      if (ts.isIdentifier(declaration.name) && declaration.initializer) declarations.set(declaration.name.text, declaration.initializer);
    }
  }
  let rule;
  for (const statement of file.statements) {
    if (ts.isExportAssignment(statement) && ts.isCallExpression(statement.expression)) {
      const candidate = statement.expression.arguments[1];
      if (candidate && ts.isObjectLiteralExpression(candidate)) rule = candidate;
    }
  }
  if (!rule) return { fixable: false, ineligible: [] };
  const named = (object, name) => object.properties.find((property) => property.name && (ts.isIdentifier(property.name) || ts.isStringLiteral(property.name)) && property.name.text === name);
  const metadata = named(rule, 'meta');
  let fixable = false;
  if (metadata && ts.isPropertyAssignment(metadata) && ts.isObjectLiteralExpression(metadata.initializer)) {
    for (const property of metadata.initializer.properties) {
      if (ts.isSpreadAssignment(property) && ts.isPropertyAccessExpression(property.expression) && property.expression.name.text === 'meta' && ts.isIdentifier(property.expression.expression)) {
        const call = declarations.get(property.expression.expression.text);
        if (call && ts.isCallExpression(call) && ts.isIdentifier(call.expression) && coreLoaders.has(call.expression.text) && call.arguments.length === 1 && ts.isStringLiteral(call.arguments[0])) {
          const coreRule = coreRules.get(call.arguments[0].text);
          if (!coreRule) throw new Error(`Unknown inherited ESLint core rule ${call.arguments[0].text}`);
          fixable = ['code', 'whitespace'].includes(coreRule.meta?.fixable);
        }
      }
      if (ts.isPropertyAssignment(property) && (ts.isIdentifier(property.name) || ts.isStringLiteral(property.name)) && property.name.text === 'fixable') fixable = ts.isStringLiteral(property.initializer) && ['code', 'whitespace'].includes(property.initializer.text);
    }
  }
  const blocked = [];
  const create = named(rule, 'create');
  if (create && ts.isMethodDeclaration(create) && create.body) {
    for (const statement of create.body.statements) {
      if (!ts.isIfStatement(statement) || !ts.isPrefixUnaryExpression(statement.expression) || statement.expression.operator !== ts.SyntaxKind.ExclamationToken || !ts.isIdentifier(statement.expression.operand)) continue;
      const gateName = statement.expression.operand.text;
      if (create.parameters.some((parameter) => ts.isIdentifier(parameter.name) && parameter.name.text === gateName)) continue;
      if (create.body.statements.some((local) => ts.isVariableStatement(local) && local.declarationList.declarations.some((declaration) => ts.isIdentifier(declaration.name) && declaration.name.text === gateName))) continue;
      const gate = declarations.get(statement.expression.operand.text);
      const branch = statement.thenStatement;
      if (!ts.isBlock(branch) || branch.statements.length !== 1 || !ts.isReturnStatement(branch.statements[0])) continue;
      const returned = branch.statements[0].expression;
      if (!returned || !ts.isObjectLiteralExpression(returned) || returned.properties.length) continue;
      if (!gate || !ts.isCallExpression(gate) || !ts.isPropertyAccessExpression(gate.expression) || gate.expression.name.text !== 'satisfies' || !ts.isIdentifier(gate.expression.expression) || !semverNames.has(gate.expression.expression.text)) continue;
      const [version, range] = gate.arguments;
      if (!version || !ts.isIdentifier(version) || !compilerVersions.has(version.text) || !range || !ts.isStringLiteral(range) || gate.arguments.length !== 2) continue;
      blocked.push(...ineligible({ svelte: range.text }, versions).map((reason) => `Rule runtime gate: ${reason}`));
    }
  }
  return { fixable, ineligible: blocked };
}

export function buildManifest(files, versions = environment, coreRules = builtinRules) {
  const get = (name) => files.get(name)?.toString('utf8');
  const json = (name) => JSON.parse(get(name));
  const cases = [];
  const rules = new Set();
  const ruleMetadataCache = new Map();
  for (const [fullPath, bytes] of files) {
    if (!fullPath.startsWith('tests/fixtures/rules/')) continue;
    const id = fullPath.slice('tests/fixtures/rules/'.length);
    const parts = id.split('/');
    const kindIndex = parts.findIndex((part) => part === 'valid' || part === 'invalid');
    if (kindIndex < 1 || parts.slice(0, -1).some((part) => part.startsWith('_')) || !isInput(parts.at(-1))) continue;
    const rule = parts.slice(0, kindIndex).join('/');
    rules.add(rule);
    const source = bytes.toString('utf8');
    if (!source) throw new Error(`Empty input: ${id}`);
    const configFile = configCandidates(fullPath).find((name) => files.has(name));
    const executableConfig = configFile && !configFile.endsWith('.json');
    const config = configFile && !executableConfig ? json(configFile) : {};
    const requirementsFile = [companion(fullPath, 'requirements.json'), path.posix.join(path.posix.dirname(fullPath), '_requirements.json')].find((name) => files.has(name));
    const requirements = requirementsFile ? json(requirementsFile) : {};
    const sourceFile = `src/rules/${rule}.ts`;
    // Only Svelte rules expose fixability here. The adapted TS integration rule
    // has no source module, and is reported as unsupported by the Rust runner.
    if (!ruleMetadataCache.has(sourceFile)) ruleMetadataCache.set(sourceFile, ruleMetadata(get(sourceFile) ?? '', versions, coreRules));
    const metadata = ruleMetadataCache.get(sourceFile);
    const fixable = metadata.fixable;
    const errorsFile = companion(fullPath, 'errors.yaml');
    const outputFile = companion(fullPath, `output${suffix(fullPath)}`);
    let errors = [];
    let output = null;
    if (parts[kindIndex] === 'invalid') {
      if (!files.has(errorsFile)) throw new Error(`Missing expected diagnostics: ${errorsFile}`);
      errors = parseYaml(get(errorsFile));
      if (!Array.isArray(errors) || errors.some((e) => typeof e.message !== 'string' || !Number.isInteger(e.line) || !Number.isInteger(e.column))) {
        throw new Error(`Invalid expected diagnostics: ${errorsFile}`);
      }
      if (fixable) {
        if (!files.has(outputFile)) throw new Error(`Missing expected fix output: ${outputFile}`);
        output = get(outputFile) === source ? null : get(outputFile);
      }
    }
    cases.push({ id, rule, kind: parts[kindIndex], filename: fullPath, configFile: configFile ?? null,
      config, executableConfig: Boolean(executableConfig), requirements, ineligible: [...ineligible(requirements, versions), ...metadata.ineligible],
      fixable, errors, output });
  }
  cases.sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  if (!cases.length) throw new Error('No fixture cases discovered');
  return { schema: 1, repository, revision, version: '3.23.0', environment: versions,
    scope: 'Raw rule fixtures. Inline, processor, config and core integration tests are not included.',
    parserDefaults: { ecmaVersion: 'latest', sourceType: 'module', globals: 'browser',
      parserOptions: { project: 'tests/fixtures/rules/tsconfig.json', extraFileExtensions: ['.svelte'], parser: { ts: '@typescript-eslint/parser', js: 'espree' } } },
    ruleCount: rules.size, cases,
    files: Object.fromEntries([...files].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([name, bytes]) => [name, digest(bytes)])),
  };
}

function main() {
  const [checkout, ...args] = process.argv.slice(2);
  if (!checkout || args.some((arg) => arg !== '--check')) throw new Error('Usage: node import.mjs <upstream-git-checkout> [--check]');
  const check = args.includes('--check');
  const git = (...args) => execFileSync('git', ['-C', checkout, ...args], { maxBuffer: 32 * 1024 * 1024 });
  if (git('rev-parse', `${revision}^{commit}`).toString().trim() !== revision) throw new Error('Pinned commit is unavailable');
  // Read Git blobs rather than the working tree. CRLF conversion and local
  // edits must not change the imported upstream expectations or their hashes.
  const entries = git('ls-tree', '-rz', revision, '--', `${packagePrefix}tests`, `${packagePrefix}src/rules`, `${packagePrefix}package.json`, 'LICENSE')
    .toString('utf8').split('\0').filter(Boolean).map((entry) => {
      const [metadata, name] = entry.split('\t');
      const [mode, type, hash] = metadata.split(' ');
      if (type !== 'blob' || !['100644', '100755'].includes(mode)) throw new Error(`Unsupported Git entry: ${name}`);
      return { name, hash };
    });
  // cat-file --batch preserves binary bytes and avoids one process per file.
  const batch = execFileSync('git', ['-C', checkout, 'cat-file', '--batch'], {
    input: entries.map((entry) => entry.hash).join('\n') + '\n', maxBuffer: 32 * 1024 * 1024,
  });
  const files = new Map();
  let offset = 0;
  for (const entry of entries) {
    const newline = batch.indexOf(10, offset);
    const size = Number(batch.subarray(offset, newline).toString().split(' ').at(-1));
    if (!Number.isSafeInteger(size)) throw new Error('Invalid cat-file response');
    const bytes = batch.subarray(newline + 1, newline + 1 + size);
    offset = newline + 1 + size + 1;
    files.set(entry.name === 'LICENSE' ? 'LICENSE' : entry.name.slice(packagePrefix.length), bytes);
  }
  const manifest = buildManifest(files);
  // Bind the package-version signal used by Oxvelte to the declared target.
  // Keep upstream's package.json above unchanged for provenance.
  files.set('tests/package.json', Buffer.from(JSON.stringify({ private: true, dependencies: { svelte: environment.svelte } }, null, 2) + '\n'));
  manifest.files['tests/package.json'] = digest(files.get('tests/package.json'));
  const manifestBytes = Buffer.from(JSON.stringify(manifest, null, 2) + '\n');
  files.set('manifest.json', manifestBytes);
  for (const [name, bytes] of files) {
    const destination = path.join(corpusRoot, ...name.split('/'));
    if (check) {
      if (!existsSync(destination) || !readFileSync(destination).equals(bytes)) throw new Error(`Import differs: ${name}`);
    } else {
      if (existsSync(destination) && !readFileSync(destination).equals(bytes)) throw new Error(`Refusing to overwrite modified import: ${name}`);
      mkdirSync(path.dirname(destination), { recursive: true });
      writeFileSync(destination, bytes);
    }
  }
  // Fail on stale files too; refreshes must deliberately account for removals.
  function verifyDirectory(dir, relative = '') {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const name = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isDirectory()) verifyDirectory(path.join(dir, entry.name), name);
      else if (!files.has(name)) throw new Error(`Unexpected imported file: ${name}`);
    }
  }
  verifyDirectory(corpusRoot);
  console.log(`${check ? 'Verified' : 'Imported'} ${manifest.cases.length} cases across ${manifest.ruleCount} rules at ${revision}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
