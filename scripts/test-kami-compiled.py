"""Compile the declared suites with the shared pinned test macro bridge.

Production views retain identical bytes. Test views change only the declared
kotoba.test macro require to kotoba.test.cljs; every input/output is hashed.
No source checkout, runtime dependency floor or assertion is rewritten.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

owner = Path(__file__).resolve().parent.parent
matrix = json.loads((owner / 'maintenance/kami-test-hosts.json').read_text())
parser = argparse.ArgumentParser()
parser.add_argument('workspace', type=Path)
parser.add_argument('--project', choices=[r['id'] for r in matrix['projects']], required=True)
parser.add_argument('--plant-failure', action='store_true')
args = parser.parse_args()
out = args.workspace.resolve()
assert '/github/workspaces/' in str(out)
out.mkdir(parents=True, exist_ok=True)
row = next(r for r in matrix['projects'] if r['id'] == args.project)
project = owner / row['path']
actual = {str(p.relative_to(project)) for p in (project / 'test').rglob('*_test.cljk')}
assert actual == {r['path'] for r in row['tests']}, 'Test host coverage changed'
for test in row['tests']:
    if not test['compiled-CLJS']:
        assert test['still-contains'] in (project / test['path']).read_text()
tests = [r['namespace'] for r in row['tests'] if r['compiled-CLJS']]
selected = {str(project / r['path']) for r in row['tests'] if r['compiled-CLJS']}
work = Path(tempfile.mkdtemp(prefix=args.project + '-compiled-', dir=out))
(work / 'tmp').mkdir()
env = {**os.environ, 'TMPDIR': str(work / 'tmp')}

def run(command, label, cwd=work, expected=0):
    p = subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True, timeout=600)
    (work / (label + '.log')).write_text(p.stdout + p.stderr)
    assert p.returncode == expected, (label, p.returncode, p.stdout[-1500:], p.stderr[-1500:])
    return p.stdout

extra = '{:deps {org.clojure/clojurescript {:mvn/version "1.11.132"} io.github.kotoba-lang/kotoba-test-cljs {:git/url "https://github.com/kotoba-lang/kotoba.git" :git/sha "f0ecb24b722664b825ab89992e9cc98c37ecaaf0" :deps/root "lib/kotoba.test/compiled"}}}'
cp = run(['clojure', '-Srepro', '-Sdeps', extra, '-Spath', '-M:test'], 'resolve', project).strip().split(os.pathsep)
views, jars, mapping = [], [], []
for index, name in enumerate(cp):
    source = (project / name).resolve()
    if source.is_file():
        jars.append(str(source))
        continue
    assert source.is_dir(), source
    origin_root = next((p for p in [source, *list(source.parents)[:4]] if (p / 'cljk-origin.edn').is_file()), None)
    origins = dict(re.findall(r'"([^\"]+\.cljk)"\s+"(\.clj[sc]?)"', (origin_root / 'cljk-origin.edn').read_text())) if origin_root else {}
    destination = work / 'sources' / str(index)
    destination.mkdir(parents=True)
    for file in source.rglob('*'):
        relative = file.relative_to(source)
        if any(v in {'.git', 'node_modules', 'target', '.cpcache', '.shadow-cljs', '.cache'} for v in relative.parts):
            continue
        if not file.is_file():
            continue
        target_name = relative.as_posix()
        if target_name.endswith('.cljk'):
            extension = origins.get(file.relative_to(origin_root).as_posix(), '.cljc') if origin_root and file.is_relative_to(origin_root) else '.cljc'
            target_name = target_name[:-5] if target_name.endswith(('.clj.cljk', '.cljs.cljk', '.cljc.cljk')) else target_name[:-5] + extension
        data = file.read_bytes()
        original = data
        transformation = None
        if str(file) in selected:
            text = data.decode()
            pattern = r'\[kotoba\.test\s+:refer\s+\[([^\]]+)\]\]'
            matches = re.findall(pattern, text)
            assert len(matches) == 1, ('Unreviewed test macro wiring', file)
            assert set(matches[0].split()) <= {'deftest', 'is', 'are', 'testing', 'async', 'use-fixtures'}
            text = re.sub(pattern, lambda m: '[kotoba.test.cljs :refer-macros [' + m[1] + ']]', text)
            data = text.encode()
            transformation = 'compiled-test-macro-require-only; assertion/body bytes unchanged'
        target = destination / target_name
        target.parent.mkdir(parents=True, exist_ok=True)
        assert not target.exists() or target.read_bytes() == data, ('Source view collision', file, target)
        target.write_bytes(data)
        mapping.append({'source': str(file), 'view': str(target.relative_to(work)),
                        'source-sha256': hashlib.sha256(original).hexdigest(),
                        'view-sha256': hashlib.sha256(data).hexdigest(), 'transformation': transformation})
    views.append(str(destination))
(work / 'source-mapping.json').write_text(json.dumps(mapping, indent=2) + '\n')
assert {r['source'] for r in mapping if r['transformation']} == selected
fixtures = project / 'test/fixtures'
fixture_mapping = []
if fixtures.is_dir():
    for file in fixtures.rglob('*'):
        if not file.is_file():
            continue
        assert not file.is_symlink()
        target = work / 'test/fixtures' / file.relative_to(fixtures)
        target.parent.mkdir(parents=True, exist_ok=True)
        data = file.read_bytes()
        target.write_bytes(data)
        fixture_mapping.append({'source': str(file), 'view': str(target.relative_to(work)),
                                'sha256': hashlib.sha256(data).hexdigest()})
(work / 'fixture-mapping.json').write_text(json.dumps(fixture_mapping, indent=2) + '\n')
(work / 'package.json').write_text('{"private":true,"type":"commonjs"}\n')
runner = work / 'runner'
(runner / 'kami/qualification').mkdir(parents=True)
views.insert(0, str(runner))
planted = '''(t/deftest planted-failure (t/is false))
(t/deftest planted-error (throw (ex-info "planted gate error" {})))
''' if args.plant_failure else ''
run_names = tests + (['kami.qualification.gate-runner'] if args.plant_failure else [])
(runner / 'kami/qualification/gate_runner.cljs').write_text('''(ns kami.qualification.gate-runner
  (:require [cljs.test :as t :include-macros true]
''' + ''.join('            [' + n + ']\n' for n in tests) + '''))
(def ended? (atom false))
(defmethod t/report [::t/default :end-run-tests] [m]
  (reset! ended? true)
  (println "qualified-summary" (pr-str (select-keys m [:test :pass :fail :error])))
  (set! (.-exitCode js/process) (if (and (pos? (:test m)) (t/successful? m)) 0 1)))
(.on js/process "beforeExit" (fn [] (when-not @ended? (set! (.-exitCode js/process) 1))))
''' + planted + '(t/run-tests ' + ' '.join("'" + n for n in run_names) + ')\n')
compiled = work / 'compiled'
compiled.mkdir()
run(['java', '-Xmx1800m', '-Djava.io.tmpdir=' + str(work / 'tmp'), '-cp', os.pathsep.join([*views, *jars]),
     'clojure.main', '-m', 'cljs.main', '--target', 'node', '--output-dir', 'compiled/out',
     '--output-to', 'compiled/tests.cjs', '-c', 'kami.qualification.gate-runner'], 'compile')
output = run(['node', 'compiled/tests.cjs'], 'Node', expected=1 if args.plant_failure else 0)
assert all('Testing ' + n in output for n in run_names)
assert re.search(r'Ran [1-9]\d* tests containing [1-9]\d* assertions\.', output)
assert ('1 failures, 1 errors.' if args.plant_failure else '0 failures, 0 errors.') in output
result = {'project': row['id'], 'host': 'real compiled CLJS / Node', 'namespaces': run_names,
          'macro-bridge': matrix['compiled-test-bridge'], 'planted-failure-refused': args.plant_failure,
          'exit': 1 if args.plant_failure else 0, 'summary': output[-1800:]}
(work / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(output[-2000:])
print('Evidence:', work)
