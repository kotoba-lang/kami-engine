import { readFileSync, realpathSync, statSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dottedID = /^kotoba(?:\.[a-z][a-z0-9]*(?:-[a-z0-9]+)*)+$/;
const existingProjects = new Map([
  ['kotoba.kami.gameplay', 'kami-gameplay'],
  ['kotoba.kami.render', 'kami-render'],
  ['kotoba.kami.physics', 'kami-physics'],
]);

function requireValue(value, message) {
  if (!value) throw new Error(message);
}

export function containedDirectory(root, path, scope = root) {
  requireValue(typeof path === 'string' && path.length > 0 && !isAbsolute(path), 'Relative source directory required');
  const target = realpathSync(resolve(root, path));
  const boundary = realpathSync(scope);
  const rel = relative(boundary, target);
  requireValue(rel !== '..' && !rel.startsWith('../') && !isAbsolute(rel), 'Source directory escapes its declared package');
  requireValue(statSync(target).isDirectory(), 'Source directory does not exist');
  return target;
}

export function createSourceRegistry(directory = repositoryRoot) {
  const root = realpathSync(directory);
  const catalog = JSON.parse(readFileSync(join(root, 'packages/catalog.json'), 'utf8'));
  const provenance = JSON.parse(readFileSync(join(root, 'maintenance/kami-source-imports.json'), 'utf8'));
  requireValue(catalog.schema === 'kotoba.kami.source-catalog.v1' && catalog.owner === 'kotoba-lang/kami-engine', 'Unknown Kami source catalog');
  requireValue(provenance.schema === 'kotoba.kami.source-imports.v1' && provenance.owner === catalog.owner, 'Unknown source provenance');
  const imported = new Map(provenance.repositories.filter(row => row['source-id']).map(row => [row['source-id'], row]));
  requireValue(imported.size === 67 && catalog.libraries.length === 70, 'Incomplete source catalog');
  const selectors = new Map();
  const ids = new Set();
  for (const entry of catalog.libraries) {
    requireValue(dottedID.test(entry.id) && !ids.has(entry.id), 'Invalid or duplicate dotted source ID');
    ids.add(entry.id);
    requireValue(entry.admission === 'not-granted', 'Source discovery cannot grant runtime admission');
    requireValue(Array.isArray(entry.aliases) && entry.aliases.length > 0 && Array.isArray(entry.sourcePaths), 'Missing exact aliases or declared source paths');
    const source = imported.get(entry.id);
    if (source) {
      const name = source.repository.split('/')[1];
      requireValue(entry.path === source.destination && entry['source-repository'] === source.repository && entry['source-commit'] === source.commit, 'Source identity does not match imported provenance');
      requireValue(JSON.stringify(entry.aliases) === JSON.stringify([name, `io.github.kotoba-lang/${name}`]), 'Legacy aliases must match the original repository exactly');
    } else {
      requireValue(existingProjects.get(entry.id) === entry.path && entry['source-repository'] === catalog.owner && entry['source-commit'] === provenance['owner-base'], 'Unknown owner project');
      requireValue(JSON.stringify(entry.aliases) === JSON.stringify([entry.path]), 'Unexpected owner project alias');
    }
    const packageRoot = containedDirectory(root, entry.path);
    requireValue(entry.sourcePaths.length > 0 || entry.kind === 'reference-programs', 'Executable source package has no source directories');
    for (const path of entry.sourcePaths) containedDirectory(packageRoot, path, packageRoot);
    for (const selector of [entry.id, ...entry.aliases]) {
      requireValue(typeof selector === 'string' && selector.length > 0 && selector.trim() === selector && !/\s/.test(selector), 'Exact selector required');
      requireValue(!selectors.has(selector), 'Ambiguous source selector');
      selectors.set(selector, entry);
    }
  }
  requireValue([...imported.keys(), ...existingProjects.keys()].every(id => ids.has(id)), 'Missing source project');
  const retired = new Map(catalog['retired-reservations'].map(row => [row['legacy-name'], row]));
  requireValue(retired.size === 7, 'Missing retired reservation documentation');
  for (const [name, row] of retired) {
    requireValue(!selectors.has(name) && row.path === `retired-reservations/${name}`, 'Retired scaffolds cannot become source packages');
    containedDirectory(root, row.path);
  }
  return {
    catalog,
    lookup(selector) {
      requireValue(typeof selector === 'string' && selector.trim() === selector && !/\s/.test(selector), 'Exact source selector required');
      if (retired.has(selector)) throw new Error(`Retired empty scaffold: ${selector}`);
      const entry = selectors.get(selector);
      requireValue(entry, `Unknown source selector: ${selector}`);
      const packageRoot = containedDirectory(root, entry.path);
      return {
        id: entry.id, aliases: entry.aliases, owner: catalog.owner,
        kind: entry.kind, path: packageRoot,
        sourcePaths: entry.sourcePaths.map(path => containedDirectory(packageRoot, path, packageRoot)),
        sourceRepository: entry['source-repository'], sourceCommit: entry['source-commit'],
        admission: 'not-granted',
      };
    },
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    const asJSON = args[0] === '--json';
    const selectors = asJSON ? args.slice(1) : args;
    requireValue(selectors.length === 1, 'Usage: node scripts/kami-sources.mjs [--json] EXACT_ID_OR_ALIAS');
    const result = createSourceRegistry().lookup(selectors[0]);
    console.log(asJSON ? JSON.stringify(result) : `${result.id}\t${result.path}\tadmission:not-granted`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
