import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
const runtime = dirname(fileURLToPath(import.meta.url));
function request(input) {
  const child = spawnSync(process.execPath, [join(runtime, 'bridge.cjs')], {
    input: JSON.stringify(input) + '\n', encoding: 'utf8',
    env: { ...process.env, OXVELTE_COMPILER_RUNTIME: runtime },
  });
  assert.equal(child.status, 0, child.stderr);
  const response = JSON.parse(child.stdout.trim());
  assert.equal(response.error, undefined);
  return response.result;
}
function compile(source, settings = {}) { return request({source,settings}); }
function callbacks(source, settings, warnings) { return request({operation:'callbacks',source,settings,warnings}); }
test('reports real Svelte warnings and compile errors', () => {
  const warn = compile('<img src="x">');
  assert.equal(warn.kind, 'warn');
  assert.equal(warn.compiler_version, '5.49.2');
  assert.equal(warn.svelte_major, 5);
  assert.equal(warn.warnings[0].code, 'a11y_missing_attribute');
  assert.equal(warn.warnings[0].start, 0);
  const error = compile('<div bind:invalid={x}></div>');
  assert.equal(error.kind, 'error');
  assert.equal(error.warnings[0].code, 'bind_invalid_name');
});
test('strips requested comments so native ignore processing can inspect warnings', () => {
  const comment = '<!-- svelte-ignore a11y_missing_attribute -->';
  const source = comment + '\n<img src="x">';
  assert.equal(compile(source).warnings.length, 0);
  const result = compile(source, { _oxvelteStripRanges: [[0, comment.length]] });
  assert.equal(result.warnings[0].code, 'a11y_missing_attribute');
  assert.equal(result.warnings[0].start, comment.length + 1);
});
test('maps TypeScript-transformed warning positions back to source', () => {
  const body = '\nlet value: number = 1;\n', source = '<script lang="ts">' + body + '</script>\n<img src="x">';
  const start = Buffer.byteLength('<script lang="ts">');
  const result = compile(source, { _oxvelteScripts: [{ start, end: start + Buffer.byteLength(body), lang: 'ts' }] });
  const warn = result.warnings.find(w => w.code === 'a11y_missing_attribute');
  assert.equal(warn.start, source.indexOf('<img'));
});
test('transforms style preprocessors and preserves unknown-style ranges', () => {
  for (const [lang, body] of [['scss', '.foo { .unused { color: red; } }'], ['less', '.foo { .unused { color: red; } }'], ['postcss', '.foo { .unused { color: red; } }'], ['stylus', '.foo\n  .unused\n    color red']]) {
    const prefix = '<div class="foo"></div><style lang="' + lang + '">';
    const source = prefix + body + '</style>', start = Buffer.byteLength(prefix);
    const result = compile(source, { _oxvelteStyles: [{ start, end: start + Buffer.byteLength(body), lang }] });
    assert.equal(result.strip_style_elements.length, 0, lang);
    assert.ok(result.warnings.some(w => w.code === 'css_unused_selector'), lang);
  }
  const result = compile('<style lang="unknown">bad syntax</style>', { _oxvelteStyles: [{ start: 22, end: 32, lang: 'unknown', element_start: 0, element_end: 40 }] });
  assert.deepEqual(result.strip_style_elements, [[0, 40]]);
});
test('missing runtime package yields an actionable error', () => {
  const child = spawnSync(process.execPath, [join(runtime, 'bridge.cjs')], { input: JSON.stringify({ source: '<div/>', filename: '/nonexistent/project/Component.svelte' }) + '\n', encoding: 'utf8', env: { ...process.env, OXVELTE_COMPILER_RUNTIME: '' } });
  assert.match(JSON.parse(child.stdout).error, /Install Svelte in the project/);
});
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
test('executes warning callbacks and preserves source-less custom reports', () => {
  const directory = mkdtempSync(join(tmpdir(), 'oxvelte-compiler-'));
  const configPath = join(directory, '_config.cjs');
  try {
    writeFileSync(configPath, 'module.exports = {languageOptions:{parserOptions:{svelteConfig:{onwarn(warning, report) {report({message:"Custom compiler warning"})}}}}}');
    const source = '<img src="x">';
    const settings = {compiler:{executableConfigPath:configPath}};
    const result = callbacks(source, settings, compile(source, settings).warnings);
    assert.equal(result.warnings[0].code, 'a11y_missing_attribute');
    assert.equal(result.warnings[0].filtered, false);
    assert.equal(result.warnings[0].report.message, 'Custom compiler warning');
    assert.equal(result.warnings[0].report.start, null);
    writeFileSync(configPath, 'module.exports = {languageOptions:{parserOptions:{svelteConfig:{warningFilter() {return false}}}}}');
    const filtered = callbacks(source, settings, compile(source, settings).warnings);
    assert.equal(filtered.warnings[0].filtered, true);
  } finally { rmSync(directory, {recursive:true, force:true}); }
});
test('Babel parser mode transforms function-bind syntax using runtime config', () => {
  const body = '\nlet obj = {}, func = () => 1; let bound = obj::func;\n';
  const prefix = '<script>', source = prefix + body + '</script>\n<img src="x">';
  const result = compile(source, { compiler: {parser:'@babel/eslint-parser'}, _oxvelteScripts:[{start:prefix.length,end:prefix.length+body.length,lang:'js'}] });
  assert.equal(result.kind, 'warn');
  assert.equal(result.warnings.find(w => w.code === 'a11y_missing_attribute').start, source.indexOf('<img'));
});

test('remaps transpiled scripts across CR and CRLF line endings', () => {
  for (const newline of ['\r', '\r\n']) {
    const body = newline + 'let value: number = 1;' + newline;
    const prefix = '<script lang="ts">', source = prefix + body + '</script>' + newline + '<img src="x">';
    const result = compile(source, { _oxvelteScripts: [{ start:prefix.length, end:prefix.length+body.length, lang:'ts' }] });
    assert.equal(result.warnings.find(w => w.code === 'a11y_missing_attribute').start, source.indexOf('<img'));
  }
});
test('configuration logs and callback stdout cannot corrupt response framing', () => {
  const directory = mkdtempSync(join(tmpdir(), 'oxvelte-compiler-log-'));
  const configPath = join(directory, '_config.cjs');
  try {
    writeFileSync(configPath, `console.log('config loaded'); module.exports = {languageOptions:{parserOptions:{svelteConfig:{onwarn(warning, report) {console.log('warning callback'); process.stdout.write('direct output\\n'); report(warning)}}}}}`);
    const child = spawnSync(process.execPath, [join(runtime, 'bridge.cjs')], {
      input: JSON.stringify({ operation:'callbacks', source:'<img src="x">', settings:{compiler:{executableConfigPath:configPath}}, warnings:compile('<img src="x">').warnings }) + '\n',
      encoding:'utf8', env:{...process.env, OXVELTE_COMPILER_RUNTIME:runtime},
    });
    assert.equal(child.status, 0);
    assert.equal(child.stdout.trim().split('\n').length, 1);
    const response = JSON.parse(child.stdout);
    assert.equal(response.result.warnings[0].code, 'a11y_missing_attribute');
    assert.equal(response.result.warnings[0].report.code, 'a11y_missing_attribute');
    assert.match(child.stderr, /config loaded/);
    assert.match(child.stderr, /warning callback/);
    assert.match(child.stderr, /direct output/);
  } finally { rmSync(directory, {recursive:true, force:true}); }
});
test('runs hooks only for warnings retained after native ignore processing', () => {
  const directory = mkdtempSync(join(tmpdir(), 'oxvelte-compiler-retained-'));
  const configPath = join(directory, '_config.cjs');
  try {
    writeFileSync(configPath, `let calls = 0; module.exports = {languageOptions:{parserOptions:{svelteConfig:{onwarn(warning, report) {console.log('HOOK CALLED'); calls++; report({...warning,message:'callback ' + calls})}}}}}`);
    const source = '<img src="first">\n<img src="second">', settings = {compiler:{executableConfigPath:configPath}};
    const rawChild = spawnSync(process.execPath, [join(runtime, 'bridge.cjs')], {input:JSON.stringify({source,settings})+'\n',encoding:'utf8',env:{...process.env,OXVELTE_COMPILER_RUNTIME:runtime}});
    assert.doesNotMatch(rawChild.stderr, /HOOK CALLED/);
    const raw = JSON.parse(rawChild.stdout).result;
    assert.equal(raw.warnings.length, 2);
    assert.equal(raw.warnings[0].report, null);
    const retained = callbacks(source, settings, raw.warnings.slice(1));
    assert.equal(retained.warnings.length, 1);
    assert.equal(retained.warnings[0].report.message, 'callback 1');
    assert.equal(retained.warnings[0].report.start, source.indexOf('<img src="second">'));
  } finally {rmSync(directory,{recursive:true,force:true});}
});
test('callbacks receive original remapped positions and compiler metadata', () => {
  const directory = mkdtempSync(join(tmpdir(), 'oxvelte-compiler-mapped-'));
  const configPath = join(directory, '_config.cjs');
  try {
    const body = '\nlet value: number = 1;\n', prefix = '<script lang="ts">';
    const source = prefix + body + '</script>\n<img src="x">';
    writeFileSync(configPath, `module.exports={languageOptions:{parserOptions:{svelteConfig:{onwarn(warning,report){if(warning.start.character!==${source.indexOf('<img')}||warning.start.line!==4||!warning.frame)throw Error('Incorrect callback location or metadata');report(warning)}}}}}`);
    const settings = {compiler:{executableConfigPath:configPath},_oxvelteScripts:[{start:prefix.length,end:prefix.length+body.length,lang:'ts'}]};
    const raw = compile(source, settings);
    const transformed = callbacks(source, settings, raw.warnings);
    assert.equal(transformed.warnings[0].report.start, source.indexOf('<img'));
  } finally {rmSync(directory,{recursive:true,force:true});}
});
test('each callback operation executes stateful hooks without caching results', () => {
  const directory = mkdtempSync(join(tmpdir(), 'oxvelte-compiler-stateful-'));
  const configPath = join(directory, '_config.cjs');
  try {
    writeFileSync(configPath, `let calls=0;module.exports={languageOptions:{parserOptions:{svelteConfig:{onwarn(warning,report){report({...warning,message:String(++calls)})}}}}}`);
    const source = '<img src="x">';
    const message = JSON.stringify({operation:'callbacks',source,settings:{compiler:{executableConfigPath:configPath}},warnings:compile(source).warnings})+'\n';
    const child = spawnSync(process.execPath,[join(runtime,'bridge.cjs')],{input:message+message,encoding:'utf8',env:{...process.env,OXVELTE_COMPILER_RUNTIME:runtime}});
    const responses = child.stdout.trim().split('\n').map(JSON.parse);
    assert.deepEqual(responses.map(response=>response.result.warnings[0].report.message),['1','2']);
  } finally {rmSync(directory,{recursive:true,force:true});}
});

test('warningFilter mutations are retained only in the reporting view', () => {
  const directory = mkdtempSync(join(tmpdir(), 'oxvelte-compiler-filter-'));
  const configPath = join(directory, '_config.cjs');
  try {
    writeFileSync(configPath, `module.exports={languageOptions:{parserOptions:{svelteConfig:{warningFilter(warning){warning.message='Filtered replacement';warning.code='custom';return true}}}}}`);
    const source = '<img src="x">', settings = {compiler:{executableConfigPath:configPath}};
    const raw = compile(source,settings);
    const transformed = callbacks(source,settings,raw.warnings);
    assert.equal(transformed.warnings[0].code,'a11y_missing_attribute');
    assert.equal(transformed.warnings[0].report.code,'custom');
    assert.equal(transformed.warnings[0].report.message,'Filtered replacement');
  } finally {rmSync(directory,{recursive:true,force:true});}
});


test('consumer with only Svelte and TypeScript needs no source-map codec dependency', async () => {
  const { mkdirSync, symlinkSync } = await import('node:fs');
  const { createRequire } = await import('node:module');
  const project = mkdtempSync(join(tmpdir(), 'oxvelte-isolated-consumer-'));
  try {
    mkdirSync(join(project, 'node_modules'));
    for (const name of ['svelte', 'typescript']) {
      symlinkSync(join(runtime, 'node_modules', name), join(project, 'node_modules', name), 'junction');
    }
    const filename = join(project, 'Component.svelte');
    assert.throws(() => createRequire(filename).resolve('@jridgewell/sourcemap-codec'), {code:'MODULE_NOT_FOUND'});
    const body = 'let value: number = 1;';
    const prefix = '<script lang="ts">';
    const source = prefix + body + '</script>\r\n<img src="x">';
    const child = spawnSync(process.execPath, [join(runtime, 'bridge.cjs')], {
      input: JSON.stringify({source, filename, settings: {_oxvelteScripts: [{start:prefix.length, end:prefix.length+body.length, lang:'ts'}]}}) + '\n',
      encoding: 'utf8', env: {...process.env, OXVELTE_COMPILER_RUNTIME: ''},
    });
    assert.equal(child.status, 0, child.stderr);
    const response = JSON.parse(child.stdout);
    assert.equal(response.error, undefined);
    assert.equal(response.result.kind, 'warn');
    assert.equal(response.result.warnings.find(warning => warning.code === 'a11y_missing_attribute').start, source.indexOf('<img'));
  } finally { rmSync(project, {recursive:true, force:true}); }
});


test('live callbacks reload edited configuration dependencies without losing unchanged state', async () => {
  const { spawn } = await import('node:child_process');
  const { createInterface } = await import('node:readline');
  const { once } = await import('node:events');
  const { statSync, utimesSync } = await import('node:fs');
  const directory = mkdtempSync(join(tmpdir(), 'oxvelte-live-config-'));
  const configPath = join(directory, '_config.cjs');
  const dependency = join(directory, 'message.cjs');
  writeFileSync(dependency, "module.exports='before';");
  writeFileSync(configPath, `let calls=0;module.exports={languageOptions:{parserOptions:{svelteConfig:{onwarn(warning,report){report({...warning,message:require('./message.cjs')+':'+(++calls)})}}}}}`);
  const child = spawn(process.execPath, [join(runtime, 'bridge.cjs')], {env:{...process.env,OXVELTE_COMPILER_RUNTIME:runtime}});
  const lines = createInterface({input:child.stdout})[Symbol.asyncIterator]();
  let stderr = ''; child.stderr.on('data', data => {stderr += data;});
  const source = '<img src="x">';
  const request = {operation:'callbacks', source, settings:{compiler:{executableConfigPath:configPath}},warnings:compile(source).warnings};
  async function next() {
    child.stdin.write(JSON.stringify(request)+'\n');
    const response = await lines.next();
    assert.equal(response.done, false, stderr);
    const decoded = JSON.parse(response.value);
    assert.equal(decoded.error, undefined);
    return decoded.result.warnings[0].report.message;
  }
  try {
    assert.equal(await next(), 'before:1');
    assert.equal(await next(), 'before:2');
    const previous = statSync(dependency);
    writeFileSync(dependency, "module.exports='after!';");
    utimesSync(dependency, previous.atime, previous.mtime);
    assert.equal(await next(), 'after!:1');
    writeFileSync(configPath, `module.exports={languageOptions:{parserOptions:{svelteConfig:{onwarn(warning,report){report({...warning,message:'new config'})}}}}}`);
    assert.equal(await next(), 'new config');
    child.stdin.end();
    await once(child, 'exit');
  } finally {
    child.kill(); rmSync(directory, {recursive:true,force:true});
  }
});


async function liveRequests(env, action) {
  const { spawn } = await import('node:child_process');
  const { createInterface } = await import('node:readline');
  const { once } = await import('node:events');
  const child = spawn(process.execPath, [join(runtime, 'bridge.cjs')], {env:{...process.env,...env}});
  const lines = createInterface({input:child.stdout})[Symbol.asyncIterator]();
  let stderr = ''; child.stderr.on('data', data => {stderr += data;});
  try {
    await action(async request => {
      child.stdin.write(JSON.stringify(request)+'\n');
      const response = await lines.next();
      assert.equal(response.done, false, stderr);
      const decoded = JSON.parse(response.value);
      assert.equal(decoded.error, undefined);
      return decoded.result;
    });
    child.stdin.end(); await once(child, 'exit');
  } finally { child.kill(); }
}

test('live Babel and PostCSS configs reload imported helpers', async () => {
  const { mkdirSync, symlinkSync } = await import('node:fs');
  const directory = mkdtempSync(join(tmpdir(), 'oxvelte-live-preprocessors-'));
  try {
    mkdirSync(join(directory, 'node_modules', '@babel'), {recursive:true});
    for (const name of ['svelte','@babel/core','postcss','postcss-load-config']) {
      symlinkSync(join(runtime,'node_modules',name),join(directory,'node_modules',name),'junction');
    }
    const helper = join(directory,'mode.cjs');
    writeFileSync(helper, 'module.exports=true;');
    writeFileSync(join(directory,'babel.config.cjs'), `const enabled=require('./mode.cjs');module.exports={plugins:[function({types:t}){return{visitor:{Program(path){if(enabled)path.node.body=[t.expressionStatement(t.awaitExpression(t.numericLiteral(1)))]}}}}]};`);
    const prefix = '<script>', body = 'let value = 1;';
    const source = prefix + body + '</script>';
    const request = {source,filename:join(directory,'Component.svelte'),settings:{compiler:{parser:'@babel/eslint-parser'},_oxvelteScripts:[{start:prefix.length,end:prefix.length+body.length,lang:'js'}]}};
    await liveRequests({OXVELTE_COMPILER_RUNTIME:directory}, async next => {
      assert.equal((await next(request)).kind, 'error');
      writeFileSync(helper, 'module.exports=false;');
      assert.equal((await next(request)).kind, 'warn');
    });
    writeFileSync(helper, "module.exports='unused';");
    writeFileSync(join(directory,'postcss.config.cjs'), `const selector=require('./mode.cjs');module.exports={plugins:[{postcssPlugin:'selector',Rule(rule){rule.selector='.'+selector}}]};`);
    const styleSource = "<div class='used'></div><style lang='postcss'>.original {color:red}</style>";
    const styleRequest = {source:styleSource,filename:join(directory,'Component.svelte'),settings:{svelte:{compileOptions:{postcss:{configFilePath:directory}}},_oxvelteStyles:[{start:styleSource.indexOf('.original'),end:styleSource.indexOf('</style>'),lang:'postcss'}]}};
    await liveRequests({OXVELTE_COMPILER_RUNTIME:directory}, async next => {
      assert.ok((await next(styleRequest)).warnings.some(warning=>warning.code==='css_unused_selector'));
      writeFileSync(helper, "module.exports='used';");
      assert.ok(!(await next(styleRequest)).warnings.some(warning=>warning.code==='css_unused_selector'));
    });
  } finally {rmSync(directory,{recursive:true,force:true});}
});
