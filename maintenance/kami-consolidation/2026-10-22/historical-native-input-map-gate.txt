#!/usr/bin/env bash
# Whole input-map component, compiled and executed by the pinned self-built Amu.
set -euo pipefail
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
: "${KOTOBA_NATIVE_HOME:?set KOTOBA_NATIVE_HOME to the reviewed Kotoba checkout}"
source "$KOTOBA_NATIVE_HOME/scripts/native-toolchain.env"
toolchain=${1:?usage: test-kami-native-input-map.sh TOOLCHAIN NEW_OUTPUT}
output=${2:?missing fresh output directory}
[ ! -e "$output" ] || { echo "output already exists" >&2; exit 73; }
toolchain=$(CDPATH= cd -- "$toolchain" && pwd -P)
mkdir -p -- "$output"
output=$(CDPATH= cd -- "$output" && pwd -P)
mkdir "$output/forbidden" "$output/objects" "$output/logs"
for name in java javac clojure clj bb node nbb kbb npm native-image python python3; do
  cat > "$output/forbidden/$name" <<'SH'
#!/bin/sh
printf '%s\n' "$0" >> "${0%/*}/used"
exit 97
SH
  chmod +x "$output/forbidden/$name"
done
export PATH="$output/forbidden:$PATH"
for name in java javac clojure clj bb node nbb kbb npm native-image python python3; do
  if "$name" > /dev/null 2>&1; then echo "denial control ran: $name" >&2; exit 1
  else code=$?; [ "$code" -eq 97 ] || exit 1; fi
done
sha() { shasum -a 256 "$1" | awk '{print $1}'; }
marker=$(sha "$output/forbidden/used")
compiler=$toolchain/artifacts/G3.bin
loader=$toolchain/host/kexe-loader
[ "$(sha "$compiler")" = "$FIXED_IMAGE_SHA256" ] || { echo "compiler pin mismatch" >&2; exit 65; }
expected_loader=$(sed -n 's/.*"loader_sha256":"\([a-f0-9]*\)".*/\1/p' "$toolchain/report.json")
[ "${#expected_loader}" -eq 64 ] && [ "$(sha "$loader")" = "$expected_loader" ] || { echo "loader proof mismatch" >&2; exit 65; }
source_root=$root/packages/kotoba.kami.engine.input-map/src
fixture_root=$root/test/native
fixture=$fixture_root/kami/native_input_map.kotoba
scopes=$output:$source_root:$fixture_root
case "$scopes" in *$'\n'*) echo "invalid source scope" >&2; exit 65 ;; esac
invoke() {
  local label=$1; shift
  [ "$(sha "$compiler")" = "$FIXED_IMAGE_SHA256" ]
  [ "$(sha "$loader")" = "$expected_loader" ]
  env -i PATH="$PATH" TMPDIR="$output" KEXE_COMMAND=1 \
    KEXE_CAP_RESOURCES_35="$scopes" KEXE_FUEL=500000000 \
    KEXE_PAIRS=16777216 KEXE_VECTORS=4194304 KEXE_VECTOR_ITEMS=134217728 \
    KEXE_STRING_POOL=268435456 KEXE_CPU_SECONDS=120 KEXE_WALL_SECONDS=120 \
    "$loader" "$compiler" 0 0 aarch64 3,35,37,38,39 -- "$@" \
    > "$output/logs/$label.stdout" 2> "$output/logs/$label.stderr"
}
invoke modules modules "$fixture" --source-path "$source_root" --source-path "$fixture_root" --unpinned
: > "$output/sources.sha256"
count=0
while IFS=' ' read -r namespace file; do
  case "$namespace" in input-map|kami.native-input-map) ;; *) echo "unexpected module $namespace" >&2; exit 65 ;; esac
  case "$file" in "$source_root/input_map.kotoba"|"$fixture") ;; *) echo "source outside declared closure" >&2; exit 65 ;; esac
  shasum -a 256 "$file" >> "$output/sources.sha256"
  args=(compile "$file" --emit-module --object-dir "$output/objects" --output "$output/objects/$namespace.kso")
  if [ "$file" = "$fixture" ]; then args+=(--entry); fi
  invoke "compile-$namespace" "${args[@]}"
  count=$((count + 1))
done < "$output/logs/modules.stdout"
[ "$count" -eq 2 ] || { echo "incomplete component closure" >&2; exit 65; }
invoke link link "$output/objects/kami.native-input-map.kso" --object-dir "$output/objects" --output "$output/component.kseed"
execute_symbol() {
  local symbol=$1 fuel=$2
  image=$output/$symbol.bin
  invoke "extract-$symbol" extract-native "$output/component.kseed" --symbol "$symbol" --output "$image"
  offset=$(sed -n 's/.*:offset \([0-9][0-9]*\).*/\1/p' "$output/logs/extract-$symbol.stdout")
  [[ "$offset" =~ ^[0-9]+$ ]] || { echo "missing native offset" >&2; exit 65; }
  env -i PATH="$PATH" KEXE_STRUCTURED_REPORT=1 KEXE_ARENA_USE=1 \
    KEXE_FUEL="$fuel" KEXE_PAIRS=1048576 KEXE_VECTORS=262144 KEXE_VECTOR_ITEMS=4194304 \
    KEXE_STRING_POOL=16777216 KEXE_CPU_SECONDS=30 KEXE_WALL_SECONDS=30 \
    "$loader" "$image" "$offset" 0 aarch64 -
}
execute_symbol main 20000000 > "$output/logs/main.stdout" 2> "$output/logs/main.stderr"
grep -Eq ':status :ok :result 255([ }]|$)' "$output/logs/main.stdout"
for symbol in malformed-touch malformed-buttons; do
  if execute_symbol "$symbol" 20000000 > "$output/logs/$symbol.stdout" 2> "$output/logs/$symbol.stderr"; then
    echo "expected native input refusal: $symbol" >&2; exit 1
  fi
  grep -q ':status :trap' "$output/logs/$symbol.stdout"
done
if execute_symbol main 1 > "$output/logs/fuel.stdout" 2> "$output/logs/fuel.stderr"; then
  echo "expected native fuel refusal" >&2; exit 1
fi
grep -q ':status :trap' "$output/logs/fuel.stdout"
shasum -a 256 -c "$output/sources.sha256" > "$output/logs/source-readback.stdout"
[ "$(sha "$output/forbidden/used")" = "$marker" ] || { echo "hosted executable invoked" >&2; exit 1; }
[ "$(sha "$compiler")" = "$FIXED_IMAGE_SHA256" ] && [ "$(sha "$loader")" = "$expected_loader" ]
printf '{"scope":"whole-input-map-component","api_count":5,"modules":2,"semantic_groups":8,"result":255,"native_input_refusals":2,"fuel_refusal":true,"hosted_denial_controls":12,"hosted_calls":0,"compiler_sha256":"%s","image_sha256":"%s","owner_main_qualified":false}\n' \
  "$FIXED_IMAGE_SHA256" "$(sha "$output/main.bin")" > "$output/report.json"
cat "$output/report.json"
