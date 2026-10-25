(ns kotoba.amenominaka
  "kami-app-amenominaka (天之御中) — Omniverse Kit-equivalent app shell.

  R1.0 path reservation per ADR-2605261800.

  This is a faithful 1:1 port of the upstream `kami-app-amenominaka` Rust
  crate, which itself is nothing but 5 identity/scope constants — a name
  reservation, not an implementation. The R1.4 gate described in the
  upstream README (extension.toml loader, extension lifecycle, extension →
  magatama Pregel cell mapping, 5-extension parity: omni.usd /
  omni.kit.app / omni.replicator.core / omni.kit.viewport / omni.timeline)
  is NOT implemented upstream and is therefore NOT invented here either.")

(def adr
  "ADR governing this path reservation."
  "ADR-2605261800")

(def phase
  "Current implementation phase."
  "R1.0-path-reservation")

(def kami-name
  "This app's name within the KAMI engine."
  "amenominaka")

(def nv-compat-target
  "The NVIDIA Omniverse surface this app shell targets compatibility with."
  "Omniverse Kit (app shell + extension system)")

(def extension-manifest-format
  "Manifest format extensions are expected to ship (Omniverse Kit format)."
  "extension.toml")

(def path-reservation
  "The 5 constants above, gathered into a single map for convenience.
  The individual `def`s are kept too (mirrors the upstream Rust `pub
  const`s 1:1); this map is purely an idiomatic-Clojure convenience for
  callers that want the whole reservation record at once."
  {:adr adr
   :phase phase
   :kami-name kami-name
   :nv-compat-target nv-compat-target
   :extension-manifest-format extension-manifest-format})
