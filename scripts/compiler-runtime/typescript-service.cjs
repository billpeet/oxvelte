// The checker sees the original script offsets, so its nodes can be reported
// directly against the component, including UTF-16 offsets and CRLF lines.
const { createRequire } = require('node:module');
const path = require('node:path');

function createSession(request) {
  const { source } = request;
  const filename = path.resolve(request.filename || '__oxvelte__.svelte');
  const runtime = process.env.OXVELTE_COMPILER_RUNTIME;
  const requireProject = createRequire(runtime
    ? path.join(path.resolve(runtime), 'package.json') : filename);
  let ts;
  try { ts = requireProject('typescript'); }
  catch (error) { throw new Error(`TypeScript type checking requires the project's typescript package: ${error.message}`); }

  const configuredProject = request.settings?.typescript?.project;
  if (configuredProject != null && typeof configuredProject !== 'string') {
    throw new Error('settings.typescript.project must be a tsconfig path.');
  }
  let configPath = configuredProject
    ? path.resolve(path.dirname(filename), configuredProject)
    : ts.findConfigFile(path.dirname(filename), ts.sys.fileExists);
  if (configPath && ts.sys.directoryExists(configPath)) configPath = path.join(configPath, 'tsconfig.json');
  let compilerOptions, projectFiles = [];
  if (configPath) {
    const config = ts.readConfigFile(configPath, ts.sys.readFile);
    if (config.error) throw configError(ts, configPath, [config.error]);
    const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configPath), undefined, configPath);
    // A Svelte-only project need not have any .ts files on disk. Its component
    // is added below, so "no inputs" is not a configuration failure here.
    const errors = parsed.errors.filter(error => error.code !== 18003);
    if (errors.length) throw configError(ts, configPath, errors);
    compilerOptions = parsed.options;
    projectFiles = parsed.fileNames;
  } else {
    compilerOptions = {
      strict: true, target: ts.ScriptTarget.Latest,
      module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler,
    };
  }
  compilerOptions = { ...compilerOptions, noEmit: true, allowJs: true, checkJs: true };

  const virtualFilename = `${filename}.ts`;
  const scriptRanges = request.scripts || [];
  const characters = source.split('');
  for (let index = 0; index < characters.length; index++) {
    if (characters[index] !== '\r' && characters[index] !== '\n') characters[index] = ' ';
  }
  for (const range of scriptRanges) {
    if (!Number.isInteger(range.start) || !Number.isInteger(range.end)
      || range.start < 0 || range.end < range.start || range.end > source.length) {
      throw new Error('TypeScript script ranges must use valid UTF-16 source offsets.');
    }
    for (let index = range.start; index < range.end; index++) characters[index] = source[index];
  }
  let virtualSource = characters.join('');
  const projections = [];
  for (const expression of request.templates || []) {
    if (!Number.isInteger(expression.start) || !Number.isInteger(expression.end)
      || expression.start < 0 || expression.end < expression.start || expression.end > source.length) {
      throw new Error('TypeScript template ranges must use valid UTF-16 source offsets.');
    }
    virtualSource += expression.kind === 'condition' ? '\nif (' : '\nvoid (';
    const start = virtualSource.length;
    virtualSource += source.slice(expression.start, expression.end);
    projections.push({ start, end: virtualSource.length, originalStart: expression.start });
    virtualSource += expression.kind === 'condition' ? ') {}' : ');';
  }
  const host = ts.createCompilerHost(compilerOptions, true);
  const normalize = filename => ts.sys.useCaseSensitiveFileNames ? path.resolve(filename) : path.resolve(filename).toLowerCase();
  const isVirtual = candidate => normalize(candidate) === normalize(virtualFilename);
  const readFile = host.readFile.bind(host), fileExists = host.fileExists.bind(host), getSourceFile = host.getSourceFile.bind(host);
  host.readFile = candidate => isVirtual(candidate) ? virtualSource : readFile(candidate);
  host.fileExists = candidate => isVirtual(candidate) || fileExists(candidate);
  host.getSourceFile = (candidate, languageVersion, onError, shouldCreateNewSourceFile) => isVirtual(candidate)
    ? ts.createSourceFile(virtualFilename, virtualSource, languageVersion, true, ts.ScriptKind.TS)
    : getSourceFile(candidate, languageVersion, onError, shouldCreateNewSourceFile);
  const program = ts.createProgram([...projectFiles, virtualFilename], compilerOptions, host);
  const checker = program.getTypeChecker();
  const sourceFile = program.getSourceFile(virtualFilename);

  const mutableRootSymbols = new Set();
  function collectBinding(name) {
    if (ts.isIdentifier(name)) {
      const symbol = checker.getSymbolAtLocation(name);
      if (symbol) mutableRootSymbols.add(symbol);
    } else for (const element of name.elements) {
      if (ts.isBindingElement(element)) collectBinding(element.name);
    }
  }
  for (const statement of sourceFile.statements) {
    if (ts.isVariableStatement(statement) && !(statement.declarationList.flags & ts.NodeFlags.Const)) {
      for (const declaration of statement.declarationList.declarations) collectBinding(declaration.name);
    }
  }
  function collectHoisted(node) {
    if (ts.isFunctionLike(node) || ts.isClassLike(node)) return;
    if (ts.isVariableDeclarationList(node) && !(node.flags & ts.NodeFlags.BlockScoped)) {
      for (const declaration of node.declarations) collectBinding(declaration.name);
    }
    ts.forEachChild(node, collectHoisted);
  }
  ts.forEachChild(sourceFile, collectHoisted);
  function isReactive(node) {
    let ancestor = node;
    while (ancestor && ancestor.parent !== sourceFile) ancestor = ancestor.parent;
    const inTemplate = node.getStart(sourceFile) >= source.length;
    if (!inTemplate && (!ancestor || !ts.isLabeledStatement(ancestor) || ancestor.label.text !== '$')) return false;
    let referencesMutable = false;
    function visit(current) {
      if (ts.isIdentifier(current)) {
        const symbol = current.parent && ts.isShorthandPropertyAssignment(current.parent)
          ? checker.getShorthandAssignmentValueSymbol(current.parent)
          : checker.getSymbolAtLocation(current);
        if (mutableRootSymbols.has(symbol)) referencesMutable = true;
      }
      if (!referencesMutable) ts.forEachChild(current, visit);
    }
    visit(node);
    return referencesMutable;
  }
  function type(node) {
    const resolved = checker.getTypeAtLocation(node);
    return checker.getBaseConstraintOfType(resolved) || resolved;
  }
  function range(node) {
    const start = typeof node.getStart === 'function' ? node.getStart(sourceFile) : node.pos ?? node.start;
    const end = node.end;
    if (start >= 0 && end <= source.length) return { start, end };
    const projection = projections.find(part => start >= part.start && end <= part.end);
    if (!projection) return null;
    return { start: projection.originalStart + start - projection.start, end: projection.originalStart + end - projection.start };
  }
  return { ts, checker, sourceFile, source, compilerOptions, program, configPath, virtualFilename, isReactive, type, range };
}

function configError(ts, filename, errors) {
  return new Error(`Unable to read TypeScript configuration ${filename}: ${errors.map(error => ts.flattenDiagnosticMessageText(error.messageText, '\n')).join('\n')}`);
}

module.exports = { createSession };
