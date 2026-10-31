#!/usr/bin/env python3
"""Run kami-engine's contract gates natively with Amu (JUN-642).

Each gate is a canonical Kotoba program under scripts/native/. This script only
stages and observes: it builds one unity source per gate (the gate's `ns` form,
scripts/native/edn_read.kotoba, the repository files the gate reads embedded as
string literals, then the gate body), runs it with the pinned Amu CLI, and
reads main's i64 result. The verdict is computed by the Kotoba program; 0 is a
pass. Every gate also runs negative controls: each mutant changes one input and
must make its gate fail with the expected code.

    python3 scripts/native_gates.py --amu path/to/amu [--gate NAME] [--no-mutants]
"""
import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NATIVE = ROOT / 'scripts' / 'native'
GAME_ROOTS = ['kami-clj-play3d/games', 'kami-clj-play/games']


def sha(data):
    return hashlib.sha256(data).hexdigest()


def lit(text):
    return '"' + text.replace('\\', '\\\\').replace('"', '\\"') + '"'


def games():
    found = []
    for root in GAME_ROOTS:
        for d in sorted((ROOT / root).iterdir()) if (ROOT / root).is_dir() else []:
            if (d / 'logic.cljk').is_file():
                found.append(f'{root}/{d.name}')
    return sorted(found)


def registry_code(c):
    if c == 1: return 'version must be 1'
    if c == 2: return 'default repo must not own native Rust implementations'
    if c == 3: return 'adapter registry requires a non-empty contracts vector'
    if 100 <= c < 200:
        return f'contract {(c - 100) // 10} is missing {[":id", ":authority", ":check", ":adapters"][(c - 100) % 10]}'
    if 200 <= c < 300: return f'contract {c - 200} must name at least one adapter'
    return f'unknown code {c}'


def wit_code(c):
    if c == 9: return 'an EDN parameter or return type has no WIT lowering'
    if 1_000_000 <= c < 2_000_000:
        return f'DRIFT: {(c - 1_000_000) // 1000} signatures only in the EDN IDL, {c % 1000} only in world.wit'
    if 2_000_000 <= c < 3_000_000: return f'{c - 2_000_000} host functions without :doc'
    if 3_000_000 <= c < 4_000_000: return f'{c - 3_000_000} host function docs missing from world.wit'
    return f'unknown code {c}'


def guest_code(c, names):
    if c == 3: return 'REFUSING TO REPORT A PASS: no games found'
    if c >= 1_000_000_000:
        g, rest = divmod(c - 1_000_000_000, 1_000_000)
        name = names[g] if g < len(names) else f'game {g}'
        return f'{name}: {rest // 1000} names the guest does not have, {rest % 1000} arity mismatches'
    return f'unknown code {c}'


GATES = {
    'adapter-registry': dict(
        source='adapter_registry.kotoba',
        inputs={'registry': 'docs/adapter-registry.edn'},
        describe=lambda c, _: registry_code(c),
        mutants={
            'version': ('docs/adapter-registry.edn', ':kami.adapter.registry/version 1', ':kami.adapter.registry/version 2', 1),
            'rust': ('docs/adapter-registry.edn', ':rust-in-default-repo? false', ':rust-in-default-repo? true', 2),
            'contracts': ('docs/adapter-registry.edn', ':kami.adapter.registry/contracts "[{', ':kami.adapter.registry/contracts "[] [{', 3),
            'missing-check': ('docs/adapter-registry.edn', ':check :adapter/conformance, ', '', 112),
            'no-adapters': ('docs/adapter-registry.edn', ':adapters [:wasm-host :browser-host :native-host]', ':adapters []', 202),
        }),
    'wit': dict(
        source='wit.kotoba',
        inputs={'idl': 'wit/kami-interface.edn', 'world': 'wit/kami-game/world.wit'},
        describe=lambda c, _: wit_code(c),
        mutants={
            'param-type': ('wit/kami-game/world.wit', 'look-at: func(entity: s64, target: s64);', 'look-at: func(entity: s64, target: s32);', 1_001_001),
            'missing-func': ('wit/kami-game/world.wit', '    add-look: func(entity: s64, dyaw: s32, dpitch: s32);\n', '', 1_001_000),
            'undocumented': ('wit/kami-interface.edn', ':doc \\"Fold a mouse / stick look delta into the pose.\\"', ':doc \\"\\"', 2_000_001),
            'unknown-type': ('wit/kami-interface.edn', '[:dyaw :f32]', '[:dyaw :f64]', 9),
            'doc-dropped': ('wit/kami-game/world.wit', '    // Read translation X — IEEE-754 f32 bit-pattern as i32.\n', '', 3_000_001),
        }),
    'guest-lint': dict(
        source='guest_lint.kotoba',
        inputs={'idl': 'wit/kami-interface.edn', 'bindings': 'wit/guest-bindings.edn'},
        games=True,
        describe=guest_code,
        mutants={
            'unknown-call': ('kami-clj-play/games/waves/logic.cljk', '(defn ', '(defn helper [] (sin 1))\n(defn ', 1_001_001_000),
            'bad-arity': ('wit/guest-bindings.edn', 'get-x               scene/get-x', 'get-x               scene/set-position', None),
        }),
}


def build_source(gate, override):
    text = (NATIVE / gate['source']).read_text()
    head, body = text.split('\n\n', 1)
    assert head.startswith('(ns '), gate['source']
    files = dict(gate['inputs'])
    read = lambda rel: override.get(rel, (ROOT / rel).read_text())
    parts = [head, '', (NATIVE / 'edn_read.kotoba').read_text()]
    for name, rel in files.items():
        parts.append(f';; staged input: {rel}\n(defn {name} [] :string {lit(read(rel))})')
    names = []
    if gate.get('games'):
        names = games()
        parts.append(f'(defn game-count [] :i64 {len(names)})')
        arms = ' '.join(f'(= i {i}) {lit(read(n + "/logic.cljk"))}\n' for i, n in enumerate(names))
        parts.append(f'(defn game-source [i :i64] :string (cond {arms} :else ""))')
    parts.append(body)
    return '\n'.join(parts), names


def run(amu, out, label, source):
    path = out / f'{label}.kotoba'
    path.write_text(source)
    check = subprocess.run([str(amu), 'check', str(path)], capture_output=True, text=True, timeout=600)
    if check.returncode != 0:
        return None, (check.stdout + check.stderr).strip()
    result = subprocess.run([str(amu), 'run', str(path)], capture_output=True, text=True, timeout=600)
    if result.returncode != 0:
        return None, f'run exited {result.returncode}: {(result.stdout + result.stderr).strip()[-400:]}'
    return int(result.stdout.strip()), None


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--amu', type=Path, required=True)
    p.add_argument('--amu-sha256')
    p.add_argument('--gate', choices=sorted(GATES), action='append')
    p.add_argument('--no-mutants', action='store_true')
    p.add_argument('--output', type=Path)
    a = p.parse_args()
    amu = a.amu.resolve()
    if a.amu_sha256:
        assert sha(amu.read_bytes()) == a.amu_sha256, 'Amu image digest mismatch'
    out = a.output or Path(tempfile.mkdtemp(prefix='kami-native-gates-'))
    out.mkdir(parents=True, exist_ok=True)
    failures, receipt = [], []
    for name in a.gate or sorted(GATES):
        gate = GATES[name]
        source, names = build_source(gate, {})
        code, err = run(amu, out, name, source)
        ok = code == 0
        line = 'ok' if ok else (err or gate['describe'](code, names))
        print(f'{"✓" if ok else "✗"} {name}: {line}')
        receipt.append(dict(gate=name, kind='gate', code=code, error=err, sourceSHA256=sha(source.encode())))
        if not ok:
            failures.append(name)
        if a.no_mutants:
            continue
        for mname, (rel, old, new, expected) in gate['mutants'].items():
            original = (ROOT / rel).read_text()
            assert original.count(old) >= 1, (name, mname, 'mutation anchor not found')
            msrc, mnames = build_source(gate, {rel: original.replace(old, new, 1)})
            mcode, merr = run(amu, out, f'{name}--{mname}', msrc)
            caught = merr is None and mcode != 0 and (expected is None or mcode == expected)
            detail = merr or gate['describe'](mcode, mnames)
            print(f'  {"✓" if caught else "✗"} negative control {mname}: {detail}')
            receipt.append(dict(gate=name, kind='mutant', mutant=mname, code=mcode, expected=expected,
                                error=merr, sourceSHA256=sha(msrc.encode())))
            if not caught:
                failures.append(f'{name}--{mname}')
    (out / 'receipt.json').write_text(json.dumps(dict(amuSHA256=sha(amu.read_bytes()), results=receipt), indent=1))
    print(f'receipt: {out / "receipt.json"}')
    if failures:
        print('FAILED:', ', '.join(failures))
        sys.exit(1)


if __name__ == '__main__':
    main()
