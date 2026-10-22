import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync, existsSync, symlinkSync, unlinkSync, renameSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createSourceRegistry, repositoryRoot } from './kami-sources.mjs';

const workspace = resolve(process.argv[2] || '');
assert(workspace.includes('/github/workspaces/') && process.argv.length === 3, 'A dedicated task scratch workspace is required');
assert(!existsSync(workspace), 'Inspect existing source-CLI test evidence; do not replay blindly');
mkdirSync(workspace, { recursive: true });
const registry = createSourceRegistry();
let lookups = 0;
for (const entry of registry.catalog.libraries) {
  const expected = registry.lookup(entry.id);
  for (const selector of [entry.id, ...entry.aliases]) {
    assert.deepEqual(registry.lookup(selector), expected);
    assert.equal(expected.admission, 'not-granted');
    assert(expected.sourcePaths.every(path => path.startsWith(expected.path)));
    lookups++;
  }
}
assert.equal(lookups, 222);
for (const row of registry.catalog['retired-reservations']) assert.throws(() => registry.lookup(row['legacy-name']), /Retired empty scaffold/);
for (const value of ['', ' kotoba.kami.contracts', 'kotoba.kami.contracts ', 'kotoba.kami.*', 'kami contracts', 'app.kotoba.contracts', 'kotoba.kami.engine-modeling', 'kotoba.kami.usdnative']) assert.throws(() => registry.lookup(value));

const fixture = join(workspace, 'owner');
mkdirSync(join(fixture, 'packages'), { recursive: true });
mkdirSync(join(fixture, 'maintenance'), { recursive: true });
const catalogFile = join(fixture, 'packages/catalog.json');
const originalCatalog = readFileSync(join(repositoryRoot, 'packages/catalog.json'), 'utf8');
writeFileSync(join(fixture, 'maintenance/kami-source-imports.json'), readFileSync(join(repositoryRoot, 'maintenance/kami-source-imports.json')));
writeFileSync(join(fixture, 'maintenance/kami-source-id-migrations.json'), readFileSync(join(repositoryRoot, 'maintenance/kami-source-id-migrations.json')));
for (const row of registry.catalog.libraries) {
  mkdirSync(join(fixture, row.path), { recursive: true });
  for (const path of row.sourcePaths) mkdirSync(join(fixture, row.path, path), { recursive: true });
}
for (const row of registry.catalog['retired-reservations']) mkdirSync(join(fixture, row.path), { recursive: true });
writeFileSync(catalogFile, originalCatalog);
assert.equal(createSourceRegistry(fixture).catalog.libraries.length, 70);
let negatives = 0;
for (const mutate of [
  catalog => { catalog.libraries[1].id = catalog.libraries[0].id; },
  catalog => { catalog.libraries[0].id = 'kotoba.kami.invalid-id'; },
  catalog => { catalog.libraries[0].id = 'kotoba.kami.invalid_id'; },
  catalog => { catalog.libraries[0].aliases[0] = 'unexpected-alias'; },
  catalog => { catalog.libraries[0].admission = 'granted'; },
  catalog => { catalog.libraries[0].sourcePaths = ['/tmp']; },
  catalog => { catalog.libraries[0].sourcePaths = ['../..']; },
  catalog => { catalog.libraries[0].path = '../outside'; },
  catalog => { catalog.libraries[0]['source-commit'] = '0'.repeat(40); },
  catalog => { catalog.libraries.push({ ...catalog.libraries[0], id: 'kotoba.kami.unreviewed' }); },
  catalog => { catalog['retired-reservations'][0].path = '../outside'; },
]) {
  const changed = JSON.parse(originalCatalog);
  mutate(changed);
  writeFileSync(catalogFile, JSON.stringify(changed));
  assert.throws(() => createSourceRegistry(fixture));
  negatives++;
}
writeFileSync(catalogFile, originalCatalog);
const first = registry.catalog.libraries.find(row => row.sourcePaths.includes('src'));
const source = join(fixture, first.path, 'src');
const saved = source + '-original';
const outside = join(workspace, 'outside');
mkdirSync(outside);
renameSync(source, saved);
symlinkSync(outside, source, 'dir');
assert.throws(() => createSourceRegistry(fixture), /escapes/);
unlinkSync(source);
renameSync(saved, source);
negatives++;
const cli = spawnSync(process.execPath, [join(repositoryRoot, 'scripts/kami-sources.mjs'), '--json', 'kami-contracts'], { cwd: workspace, encoding: 'utf8' });
assert.equal(cli.status, 0, cli.stderr);
const descriptor = JSON.parse(cli.stdout);
assert.equal(descriptor.id, 'kotoba.kami.contracts');
assert.equal(descriptor.admission, 'not-granted');
assert(descriptor.path.startsWith(join(repositoryRoot, 'packages') + '/'));
const result = { exactLookups: lookups, sourceIDs: 70, exactAliases: 152, explicitDraftIDMigrations: 15, retiredScaffoldRefusals: 7, catalogAndSymlinkRefusals: negatives, externalCWD: true, admission: 'not-granted' };
writeFileSync(join(workspace, 'results.json'), JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify(result));
