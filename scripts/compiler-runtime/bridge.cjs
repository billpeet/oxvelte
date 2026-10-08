// Compiles components with the project's Svelte installation. UTF-16 positions are
// kept until the Rust boundary, including positions remapped through preprocessors.
const { createRequire } = require('node:module');
const path = require('node:path');
const readline = require('node:readline');
// User configuration and preprocessors may log while compiling. Keep their
// output off the line-delimited response channel, including direct stdout writes.
const protocolWrite = process.stdout.write.bind(process.stdout);
const diagnosticWrite = process.stderr.write.bind(process.stderr);
process.stdout.write = (...args) => diagnosticWrite(...args);
function resolver(filename) {
  const root = process.env.OXVELTE_COMPILER_RUNTIME;
  const anchor = root ? path.join(path.resolve(root), 'package.json') : path.resolve(filename || path.join(process.cwd(), '__oxvelte__.svelte'));
  return createRequire(anchor);
}
function lineStarts(text) {
  const starts = [0];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '\r') { if (text[i + 1] === '\n') i++; starts.push(i + 1); }
    else if (text[i] === '\n') starts.push(i + 1);
  }
  return starts;
}
function indexAt(text, line, column) {
  const starts = lineStarts(text);
  return Math.min(text.length, (starts[line - 1] ?? text.length) + column);
}
function positionAt(text, index) {
  const starts = lineStarts(text); let line = 1;
  while (line < starts.length && starts[line] <= index) line++;
  return { line, column: index - starts[line - 1], character: index };
}
function sourceMapRemap(output, input, mappings, decode) {
  const lines = decode(mappings);
  return (index) => {
    const p = positionAt(output, index); let line = p.line - 1; let maps = lines[line];
    if (!maps?.length) {
      while (line >= 0 && !lines[line]?.length) line--;
      if (line < 0) return 0;
      const m = lines[line].at(-1); return indexAt(input, m[2] + 1, m[3]);
    }
    let m = maps[0]; for (const entry of maps) { if (entry[0] > p.column) break; m = entry; }
    return indexAt(input, m[2] + 1, m[3] + p.column - m[0]);
  };
}
function byteToIndex(source, byte) { return Buffer.from(source).subarray(0, byte).toString('utf8').length; }
function loadConfig(req, cfg) {
  let config = cfg.svelteConfig || {};
  if (cfg.executableConfigPath) {
    const exported = req(path.resolve(cfg.executableConfigPath));
    const entry = Array.isArray(exported) ? exported.find(x => x.languageOptions?.parserOptions?.svelteConfig) : exported;
    config = entry?.languageOptions?.parserOptions?.svelteConfig || {};
  }
  return config;
}
function applyCallbacks(request) {
  const { source, filename } = request, settings = request.settings || {};
  const config = loadConfig(resolver(filename), settings.compiler || {});
  function report(value) {
    const point = value => value ? (value.character ?? indexAt(source, value.line, value.column)) : null;
    let start = point(value.start), end = point(value.end);
    start ??= end; end ??= start;
    return {message:value.message, code:value.code || null, start, end};
  }
  return {warnings:request.warnings.map(raw => {
    raw.filtered = false; raw.report = null;
    if (!raw.code) return raw;
    const value = {...(raw.metadata || {}), message:raw.message, code:raw.code};
    delete value.start; delete value.end;
    if (raw.start != null) value.start = positionAt(source, raw.start);
    if (raw.end != null) value.end = positionAt(source, raw.end);
    if (config.warningFilter) {
      raw.filtered = !config.warningFilter(value);
      if (!raw.filtered) raw.report = report(value);
    }
    else if (config.onwarn) {
      let replacement = null; config.onwarn(value, w => {replacement = w;});
      raw.filtered = !replacement; if (replacement) raw.report = report(replacement);
    }
    return raw;
  })};
}
async function run(request) {
  const { source, filename } = request; const settings = request.settings || {};
  const req = resolver(filename); let compiler;
  try { compiler = req('svelte/compiler'); }
  catch (e) { throw new Error('Cannot resolve svelte/compiler for ' + (filename || process.cwd()) + '. Install Svelte in the project or set OXVELTE_COMPILER_RUNTIME to a runtime directory. ' + e.message); }
  const cfg = settings.compiler || {};
  const config = loadConfig(req, cfg);
  let text = source;
  const strip = [...(settings._oxvelteStripRanges || [])];
  const styles = settings._oxvelteStyles || [];
  const transforms = [], strippedStyles = [];
  for (const style of styles) {
    const lang = (style.lang || '').toLowerCase();
    if (!lang || lang === 'css') continue;
    const start = byteToIndex(source, style.start), end = byteToIndex(source, style.end);
    const input = source.slice(start, end); let output, mappings;
    try {
      if (lang === 'scss' || lang === 'sass') {
        const result = req('sass').compileString(input, { sourceMap: true, syntax: lang === 'sass' ? 'indented' : undefined });
        output = result.css; mappings = result.sourceMap.mappings;
      } else if (lang === 'less') {
        const result = await req('less').render(input, { sourceMap: {}, syncImport: true, filename: (filename || '__oxvelte__') + '.less', lint: false });
        output = result.css; mappings = JSON.parse(result.map).mappings;
      } else if (lang === 'styl' || lang === 'stylus') {
        const styl = req('stylus')(input, { filename: (filename || '__oxvelte__') + '.stylus' }).set('sourcemap', {});
        output = await new Promise((resolve, reject) => styl.render((error, css) => error ? reject(error) : resolve(css)));
        mappings = styl.sourcemap.mappings;
      } else if (lang === 'postcss' || lang === 'pcss') {
        const setting = settings.svelte?.compileOptions?.postcss;
        if (setting === false) throw new Error('PostCSS disabled');
        const configPath = setting?.configFilePath || process.env.OXVELTE_COMPILER_RUNTIME || process.cwd();
        const config = await req('postcss-load-config')({ cwd: process.cwd(), from: (filename || '__oxvelte__') + '.css' }, configPath);
        const result = req('postcss')(config.plugins).process(input, { ...config.options, map: { inline: false } });
        output = result.content; mappings = result.map.toJSON().mappings;
      }
    } catch { /* Unsupported or unavailable preprocessors follow upstream's stripped-style path. */ }
    if (output == null) {
      strip.push([style.start, style.end]);
      strippedStyles.push([style.element_start ?? style.start, style.element_end ?? style.end]);
    } else {
      transforms.push({ start, end, output: output + '\n', remap: sourceMapRemap(output + '\n', input, mappings, req('@jridgewell/sourcemap-codec').decode) });
    }
  }
  const chars = text.split('');
  for (const [begin, end] of strip) {
    const a = byteToIndex(source, begin), b = byteToIndex(source, end);
    for (let i = a; i < b; i++) if (!/[\t\n\r ]/.test(chars[i])) chars[i] = ' ';
  }
  text = chars.join('');
  for (const script of settings._oxvelteScripts || []) {
    const lang = (script.lang || '').toLowerCase();
    if (lang !== 'ts' && lang !== 'typescript' && cfg.parser !== '@babel/eslint-parser') continue;
    const start = byteToIndex(source, script.start), end = byteToIndex(source, script.end);
    const input = text.slice(start, end);
    let output, mappings;
    if (lang === 'ts' || lang === 'typescript') {
      let ts; try { ts = req('typescript'); } catch { throw new Error('TypeScript script compilation requires the project typescript package.'); }
      const result = ts.transpileModule(input, { reportDiagnostics: false, compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext, importsNotUsedAsValues: ts.ImportsNotUsedAsValues.Preserve, preserveValueImports: true, verbatimModuleSyntax: true, sourceMap: true } });
      output = result.outputText; mappings = JSON.parse(result.sourceMapText).mappings;
    } else {
      const babel = req('@babel/core');
      const result = babel.transformSync(input, { sourceType: 'module', sourceMaps: true, minified: false, ast: false, code: true, cwd: process.env.OXVELTE_COMPILER_RUNTIME || process.cwd() });
      output = result.code; mappings = result.map.mappings;
    }
    const decode = req('@jridgewell/sourcemap-codec').decode;
    transforms.push({ start, end, output: output + '\n', remap: sourceMapRemap(output + '\n', input, mappings, decode) });
  }
  const maps = []; let code = '', cursor = 0;
  for (const transform of transforms.sort((a,b) => a.start-b.start)) {
    const begin = code.length, original = cursor; code += text.slice(cursor, transform.start);
    maps.push({ start: begin, end: code.length, map: i => i - begin + original });
    const at = code.length; code += transform.output;
    maps.push({ start: at, end: code.length, map: i => transform.start + transform.remap(i - at) });
    cursor = transform.end;
  }
  const tailStart = code.length, tailOriginal = cursor; code += text.slice(cursor);
  maps.push({ start: tailStart, end: code.length + 1, map: i => i - tailStart + tailOriginal });
  function remap(i) { const region = maps.find(m => m.start <= i && i < m.end); return region ? region.map(i) : 0; }
  function warning(value) {
    const originalIndex = point => {
      if (!point) return null;
      return transforms.length ? remap(indexAt(code, point.line, point.column)) : (point.character ?? indexAt(code, point.line, point.column));
    };
    let start = originalIndex(value.start), end = originalIndex(value.end);
    // End positions belong to the preceding transformed region.
    if (transforms.length && value.end) end = remap(indexAt(code, value.end.line, value.end.column) - 1) + 1;
    start ??= end; end ??= start;
    return { message: value.message, code: value.code || null, start, end };
  }
  let result;
  try {
    const options = { generate: false };
    if (String(compiler.VERSION).startsWith('5.')) options.experimental = { async: config.compilerOptions?.experimental?.async };
    if (settings._oxvelteCustomElement) options.customElement = true;
    result = { kind: 'warn', warnings: compiler.compile(code, options).warnings };
  } catch (e) { result = { kind: 'error', warnings: [e] }; }
  return { compiler_version: compiler.VERSION, svelte_major: Number.parseInt(compiler.VERSION, 10), kind: result.kind, warnings: result.warnings.map(value => {
    const raw = warning(value); raw.metadata = {...value}; raw.filtered = false; raw.report = null;
    return raw;
  }), strip_style_elements: strippedStyles, unused_ignores: [], ignore_items: [] };
}
const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
rl.on('line', async line => {
  try { const request = JSON.parse(line); const result = request.operation === 'callbacks' ? applyCallbacks(request) : await run(request); protocolWrite(JSON.stringify({ result }) + '\n'); }
  catch (e) { protocolWrite(JSON.stringify({ error: e.message }) + '\n'); }
});






