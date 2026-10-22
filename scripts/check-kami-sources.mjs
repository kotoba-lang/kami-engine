import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, lstatSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { createSourceRegistry, repositoryRoot } from './kami-sources.mjs';

function requireValue(value, message) { if (!value) throw new Error(message); }
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const { catalog } = createSourceRegistry();
const metadata = JSON.parse(readFileSync(join(repositoryRoot, 'maintenance/kami-source-imports.json'), 'utf8'));
requireValue(metadata.repositories.length === 74 && metadata['source-imports'] === 67 && metadata['retired-doc-imports'] === 7, 'Incomplete source provenance');
let files = 0;
for (const row of metadata.repositories) {
  requireValue(/^[0-9a-f]{40}$/.test(row.commit) && /^[0-9a-f]{40}$/.test(row.tree), 'Immutable source commit/tree required');
  const root = resolve(repositoryRoot, row.destination);
  requireValue(root.startsWith(repositoryRoot + '/') && !row.destination.split('/').includes('..'), 'Imported path escape');
  const changes = new Map([...(row['config-changes'] || []), ...(row['source-and-test-changes'] || [])].map(change => [change.path, change]));
  const originals = new Map(row['original-files'].map(file => [file.path, file]));
  const additions = new Map((row['added-files'] || []).map(file => [file.path, file]));
  requireValue(originals.size === row['original-files'].length && originals.has('LICENSE'), 'Original files or license missing');
  for (const [path, file] of originals) {
    const relativePath = `${row.destination}/${path}`;
    const change = changes.get(relativePath);
    if (change) requireValue(change['original-sha256'] === file.sha256, 'Changed-file preimage mismatch');
    const disk = join(root, path);
    const status = lstatSync(disk);
    requireValue(status.isFile() && !status.isSymbolicLink(), `Original blob is not a regular file: ${relativePath}`);
    const expectedMode = file.mode === '100755' ? 0o755 : 0o644;
    requireValue((status.mode & 0o777) === expectedMode, `Original mode mismatch: ${relativePath}`);
    requireValue(hash(readFileSync(disk)) === (change ? change['rewritten-sha256'] : file.sha256), `Unreviewed source drift: ${relativePath}`);
    files++;
  }
  requireValue([...changes.keys()].every(path => originals.has(path.slice(row.destination.length + 1))), 'Unbound source change');
  function walk(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const disk = join(directory, entry.name);
      const path = relative(root, disk);
      if (entry.isDirectory()) {
        if (['.cpcache', '.nbb', '.clj-kondo'].includes(entry.name)) continue;
        walk(disk);
      } else {
        requireValue(entry.isFile() && !entry.isSymbolicLink(), `Unexpected source link: ${disk}`);
        if (!originals.has(path)) {
          const added = additions.get(path);
          requireValue(added && hash(readFileSync(disk)) === added.sha256, `Unrecorded added source file: ${disk}`);
        }
      }
    }
  }
  walk(root);
}
requireValue(files === 1677, 'Original tracked file coverage changed');
requireValue(catalog.libraries.length === 70, 'Catalog coverage changed');
for (const change of metadata['owner-test-changes'] || []) {
  requireValue(/^(kami-gameplay|kami-render|kami-physics)\/test\//.test(change.path), 'Owner production source cannot be changed by a test-host exception');
  requireValue(change.kind === 'existing-test-host-compatibility' && change.reason, 'Unexplained owner test change');
  const file = join(repositoryRoot, change.path);
  requireValue(lstatSync(file).isFile() && !lstatSync(file).isSymbolicLink(), 'Owner test path changed type');
  requireValue(hash(readFileSync(file)) === change['rewritten-sha256'], 'Unreviewed owner test drift');
}
console.log(`Kami sources: ${files} immutable original file preimages, 67 imported + 3 existing projects, 7 retired doc trees verified`);
