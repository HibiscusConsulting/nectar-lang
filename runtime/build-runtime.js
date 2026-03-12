#!/usr/bin/env node
// build-runtime.js — Assembles a tree-shaken Nectar runtime
//
// Usage: node build-runtime.js --modules core,seo,form --output dist/nectar-runtime.js
//
// The compiler calls this during `nectar build` to produce a minimal runtime
// containing only the modules the program actually uses.

const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
let modules = ['core']; // core is always included
let outputPath = 'nectar-runtime.bundled.js';

for (let i = 0; i < args.length; i++) {
  if (args[i] === '--modules' && args[i + 1]) {
    modules = ['core', ...args[i + 1].split(',').filter(m => m !== 'core')];
    i++;
  } else if (args[i] === '--output' && args[i + 1]) {
    outputPath = args[i + 1];
    i++;
  }
}

// Deduplicate
modules = [...new Set(modules)];

const modulesDir = path.join(__dirname, 'modules');
const availableModules = fs.readdirSync(modulesDir)
  .filter(f => f.endsWith('.js'))
  .map(f => f.replace('.js', ''));

// Validate requested modules exist
for (const mod of modules) {
  if (!availableModules.includes(mod)) {
    console.error(`Error: unknown module "${mod}". Available: ${availableModules.join(', ')}`);
    process.exit(1);
  }
}

let output = '// Nectar Runtime (tree-shaken)\n';
output += '// Modules: ' + modules.join(', ') + '\n';
output += '// Generated: ' + new Date().toISOString() + '\n\n';

// Read and concatenate modules
for (const mod of modules) {
  const filePath = path.join(modulesDir, `${mod}.js`);
  if (fs.existsSync(filePath)) {
    const content = fs.readFileSync(filePath, 'utf8');
    output += `// --- ${mod} module ---\n`;
    // Strip the module.exports line since we're bundling
    const stripped = content
      .replace(/^\/\/ runtime\/modules\/\w+\.js.*\n/m, '')
      .replace(/if \(typeof module !== "undefined"\) module\.exports = \w+;?\n?/g, '');
    output += stripped + '\n\n';
  }
}

// Write the WASM import assembly
output += '// --- WASM Import Assembly ---\n';
output += 'function buildWasmImports(memory) {\n';
output += '  const imports = {};\n';
for (const mod of modules) {
  const varName = mod + 'Module';
  output += `  if (typeof ${varName} !== "undefined" && ${varName}.wasmImports) {\n`;
  output += `    for (const [ns, fns] of Object.entries(${varName}.wasmImports)) {\n`;
  output += `      imports[ns] = Object.assign(imports[ns] || {}, fns);\n`;
  output += `    }\n`;
  output += `  }\n`;
}
output += '  return imports;\n';
output += '}\n\n';

output += 'if (typeof module !== "undefined") module.exports = { buildWasmImports };\n';
output += 'if (typeof window !== "undefined") window.NectarBuildImports = buildWasmImports;\n';

// Ensure output directory exists
const outDir = path.dirname(outputPath);
if (outDir && outDir !== '.' && !fs.existsSync(outDir)) {
  fs.mkdirSync(outDir, { recursive: true });
}

fs.writeFileSync(outputPath, output);

const fullSize = availableModules.reduce((total, mod) => {
  const p = path.join(modulesDir, `${mod}.js`);
  return total + (fs.existsSync(p) ? fs.statSync(p).size : 0);
}, 0);

console.log(`Runtime written to ${outputPath}`);
console.log(`  Modules: ${modules.length}/${availableModules.length} (${modules.join(', ')})`);
console.log(`  Size: ${output.length} bytes (full runtime: ${fullSize} bytes)`);
console.log(`  Savings: ${Math.round((1 - output.length / fullSize) * 100)}%`);
