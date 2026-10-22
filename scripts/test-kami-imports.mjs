import assert from 'node:assert/strict';
import {mkdirSync, existsSync, writeFileSync, realpathSync} from 'node:fs';
import {join, resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {createSourceRegistry, repositoryRoot} from './kami-sources.mjs';

const workspace = resolve(process.argv[2] || '');
assert(process.argv.length === 3 && workspace.includes('/github/workspaces/') && !existsSync(workspace), 'Use a new dedicated evidence directory');
assert(process.env.KOTOBA_CLI, 'Set KOTOBA_CLI to the canonical Kotoba source launcher');
const cli = realpathSync(process.env.KOTOBA_CLI);
mkdirSync(workspace, {recursive: true});
const registry = createSourceRegistry();
const cases = [
  {selector: 'kotoba.kami.engine.modeling', fixture: 'modeling.cljk', check: value => {assert.equal(value.revision, 'k1-urrmhc'); assert.equal(value['round-trip'], true);}},
  {selector: 'kotoba.kami.mangaka.sceneAuthor', fixture: 'scene_author.cljk', check: value => assert.equal(value.fov, 35.25)},
];
const results = [];
for (const test of cases) {
  const source = registry.lookup(test.selector);
  const args = ['run', join(repositoryRoot, 'test/imports', test.fixture), '--runtime', 'sci', '--library', 'kotoba.text', '--library', 'kotoba.edn'];
  for (const path of source.sourcePaths) args.push('--source-path', path);
  args.push('--json');
  const run = spawnSync(cli, args, {cwd: workspace, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024});
  writeFileSync(join(workspace, test.fixture + '.stdout'), run.stdout);
  writeFileSync(join(workspace, test.fixture + '.stderr'), run.stderr);
  assert.equal(run.status, 0, run.stderr);
  const result = JSON.parse(run.stdout);
  assert.equal(result['kotoba.cli/ok?'], true);
  const value = result['kotoba.cli/data'].value;
  test.check(value);
  const origins = Object.values(value.origins);
  assert(origins.length > 0 && origins.every(path => path.startsWith(source.path + '/src/')), 'Source origin escaped the selected owner package');
  results.push({sourceID: source.id, sourceCommit: source.sourceCommit, origins, passed: true, host: 'public Kotoba CLI / explicit SCI source compatibility'});
}
writeFileSync(join(workspace, 'results.json'), JSON.stringify(results, null, 2) + '\n');
console.log(JSON.stringify({projects: results.length, sourceOrigins: results.reduce((n, row) => n + row.origins.length, 0), passed: true, admission: 'not-granted'}));
