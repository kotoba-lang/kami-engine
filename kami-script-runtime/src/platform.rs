//! platform — the ADR-0037 packaging matrix as executable data.
//!
//! The cross-platform decisions (does this target ban JIT? which WASM host,
//! texture format, renderer backend, and default input does it use?) were prose
//! in ADR-0037; this module makes them a single source of truth the packaging
//! tooling (`bb kami host/package`) and CI consume. Pure data + pure functions,
//! no platform deps — fully unit-tested on any host.
//!
//! The load-bearing invariant: **iOS, PS5, and Switch forbid runtime codegen**,
//! so they must use the `wasmi` (interpreter) host, never `wasmtime` (JIT). The
//! `PlatformSpec` below encodes that, and the tests assert it can't regress.

/// A shippable target of the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Web,
    Mac,
    Linux,
    Windows,
    Ios,
    Android,
    Ps5,
    Switch,
}

/// Which WASM execution host drives the compiled game logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicHost {
    /// The browser's own WASM engine runs the guest (no embedded runtime).
    BrowserWasm,
    /// `wasmtime` — JIT. Allowed only where W^X is not enforced.
    Wasmtime,
    /// `wasmi` — pure interpreter, no runtime codegen. Required on no-JIT targets.
    Wasmi,
}

/// Compressed texture family baked into the KTX2 asset variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexFmt {
    /// Pick per-GPU at load (browser: BCn or ASTC via KTX2 transcode).
    Ktx2Auto,
    /// Desktop / PS5 GPUs.
    Ktx2Bcn,
    /// Mobile / Switch GPUs.
    Ktx2Astc,
}

/// The wgpu (or console) graphics backend `kami-render` bootstraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBackend {
    WebGpu,
    Metal,
    Vulkan,
    Dx12,
    /// PS5 (GNM/AGC) or Switch (NVN) — NDA backend behind `for_console_surface`.
    Console,
}

/// Default input profile when the target has no keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDefault {
    Keyboard,
    Touch,
    Gamepad,
}

/// The full per-target packaging decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformSpec {
    pub target: Target,
    /// Whether the OS/SDK permits runtime code generation (JIT).
    pub jit_allowed: bool,
    pub logic: LogicHost,
    pub tex: TexFmt,
    pub render: RenderBackend,
    pub input: InputDefault,
    /// True when the renderer needs an NDA console backend not in this repo.
    pub console_seam: bool,
}

impl Target {
    /// All shippable targets (for tooling that sweeps every platform).
    pub fn all() -> [Target; 8] {
        use Target::*;
        [Web, Mac, Linux, Windows, Ios, Android, Ps5, Switch]
    }

    /// Lowercase tag used on the `bb kami --target <tag>` CLI and in `:platform`.
    pub fn tag(self) -> &'static str {
        use Target::*;
        match self {
            Web => "web",
            Mac => "mac",
            Linux => "linux",
            Windows => "windows",
            Ios => "ios",
            Android => "android",
            Ps5 => "ps5",
            Switch => "switch",
        }
    }

    pub fn from_tag(s: &str) -> Option<Target> {
        Target::all().into_iter().find(|t| t.tag() == s)
    }

    /// The rustc target triple the host crate cross-compiles to. `None` for PS5 /
    /// Switch — their toolchains are NDA console SDKs, not public rustc targets.
    pub fn triple(self) -> Option<&'static str> {
        use Target::*;
        match self {
            Web => Some("wasm32-unknown-unknown"),
            Mac => Some("aarch64-apple-darwin"),
            Linux => Some("x86_64-unknown-linux-gnu"),
            Windows => Some("x86_64-pc-windows-msvc"),
            Ios => Some("aarch64-apple-ios"),
            Android => Some("aarch64-linux-android"),
            Ps5 | Switch => None,
        }
    }

    /// The packaging decision for this target — the ADR-0037 matrix, in code.
    pub fn spec(self) -> PlatformSpec {
        use InputDefault::*;
        use LogicHost::*;
        use RenderBackend::*;
        use Target::*;
        use TexFmt::*;
        // (jit_allowed, logic, tex, render, input)
        let (jit, logic, tex, render, input) = match self {
            Web => (true, BrowserWasm, Ktx2Auto, WebGpu, Keyboard),
            Mac => (true, Wasmtime, Ktx2Bcn, Metal, Keyboard),
            Linux => (true, Wasmtime, Ktx2Bcn, Vulkan, Keyboard),
            Windows => (true, Wasmtime, Ktx2Bcn, Dx12, Keyboard),
            // App Store / console SDKs forbid JIT → wasmi.
            Ios => (false, Wasmi, Ktx2Astc, Metal, Touch),
            Android => (true, Wasmtime, Ktx2Astc, Vulkan, Touch),
            Ps5 => (false, Wasmi, Ktx2Bcn, Console, Gamepad),
            Switch => (false, Wasmi, Ktx2Astc, Console, Gamepad),
        };
        PlatformSpec {
            target: self,
            jit_allowed: jit,
            logic,
            tex,
            render,
            input,
            console_seam: render == Console,
        }
    }
}

impl PlatformSpec {
    /// The cargo feature the native host crate is built with for this target.
    /// `None` for web (the browser hosts the guest; no runtime is linked).
    pub fn host_feature(self) -> Option<&'static str> {
        match self.logic {
            LogicHost::BrowserWasm => None,
            LogicHost::Wasmtime => Some("backend-wasmtime"),
            LogicHost::Wasmi => Some("backend-wasmi"),
        }
    }
}

// Lowercase labels for CLI tables and the EDN spec — one definition (the `kami`
// bin used to duplicate these).
impl LogicHost {
    pub fn label(self) -> &'static str {
        match self {
            LogicHost::BrowserWasm => "browser",
            LogicHost::Wasmtime => "wasmtime",
            LogicHost::Wasmi => "wasmi",
        }
    }
}
impl TexFmt {
    pub fn label(self) -> &'static str {
        match self {
            TexFmt::Ktx2Auto => "ktx2-auto",
            TexFmt::Ktx2Bcn => "ktx2-bcn",
            TexFmt::Ktx2Astc => "ktx2-astc",
        }
    }
}
impl RenderBackend {
    pub fn label(self) -> &'static str {
        match self {
            RenderBackend::WebGpu => "webgpu",
            RenderBackend::Metal => "metal",
            RenderBackend::Vulkan => "vulkan",
            RenderBackend::Dx12 => "dx12",
            RenderBackend::Console => "console",
        }
    }
}
impl InputDefault {
    pub fn label(self) -> &'static str {
        match self {
            InputDefault::Keyboard => "keyboard",
            InputDefault::Touch => "touch",
            InputDefault::Gamepad => "gamepad",
        }
    }
}

impl Target {
    /// The packaging decision as a machine-readable EDN map — the contract the
    /// `bb kami` orchestration reads via `clojure.edn/read-string`. One source of
    /// truth (not re-built by hand in the bin), and round-trip-tested below.
    pub fn spec_edn(self) -> String {
        let s = self.spec();
        let q = |o: Option<&str>| o.map(|v| format!("\"{v}\"")).unwrap_or_else(|| "nil".into());
        format!(
            "{{:target \"{}\" :jit {} :host \"{}\" :feature {} :tex \"{}\" :render \"{}\" :input \"{}\" :triple {} :console-seam {}}}",
            self.tag(),
            s.jit_allowed,
            s.logic.label(),
            q(s.host_feature()),
            s.tex.label(),
            s.render.label(),
            s.input.label(),
            q(self.triple()),
            s.console_seam,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_jit_targets_use_wasmi() {
        // The whole reason wasmi exists: iOS/PS5/Switch ban runtime codegen.
        for t in [Target::Ios, Target::Ps5, Target::Switch] {
            let s = t.spec();
            assert!(!s.jit_allowed, "{:?} must be no-JIT", t);
            assert_eq!(s.logic, LogicHost::Wasmi, "{:?} must host on wasmi", t);
            assert_eq!(s.host_feature(), Some("backend-wasmi"));
        }
    }

    #[test]
    fn jit_targets_use_wasmtime_or_browser() {
        for t in [Target::Mac, Target::Linux, Target::Windows, Target::Android] {
            let s = t.spec();
            assert!(s.jit_allowed);
            assert_eq!(s.logic, LogicHost::Wasmtime);
        }
        // Web is JIT-capable but the browser hosts the guest itself.
        let web = Target::Web.spec();
        assert_eq!(web.logic, LogicHost::BrowserWasm);
        assert_eq!(web.host_feature(), None);
    }

    #[test]
    fn only_consoles_need_the_nda_seam() {
        for t in Target::all() {
            let needs = t.spec().console_seam;
            let expected = matches!(t, Target::Ps5 | Target::Switch);
            assert_eq!(needs, expected, "{:?} console_seam", t);
        }
    }

    #[test]
    fn texture_family_matches_gpu_class() {
        // Mobile + Switch ship ASTC; desktop + PS5 ship BCn; web transcodes.
        assert_eq!(Target::Ios.spec().tex, TexFmt::Ktx2Astc);
        assert_eq!(Target::Android.spec().tex, TexFmt::Ktx2Astc);
        assert_eq!(Target::Switch.spec().tex, TexFmt::Ktx2Astc);
        assert_eq!(Target::Mac.spec().tex, TexFmt::Ktx2Bcn);
        assert_eq!(Target::Ps5.spec().tex, TexFmt::Ktx2Bcn);
        assert_eq!(Target::Web.spec().tex, TexFmt::Ktx2Auto);
    }

    #[test]
    fn consoles_have_no_public_triple() {
        assert!(Target::Ps5.triple().is_none());
        assert!(Target::Switch.triple().is_none());
        for t in [Target::Web, Target::Mac, Target::Ios, Target::Android] {
            assert!(t.triple().is_some(), "{:?} needs a rustc triple", t);
        }
    }

    #[test]
    fn tag_roundtrips() {
        for t in Target::all() {
            assert_eq!(Target::from_tag(t.tag()), Some(t));
        }
        assert_eq!(Target::from_tag("n64"), None);
    }

    #[test]
    fn spec_edn_is_valid_and_matches_the_matrix() {
        // The `bb kami` pipeline parses `kami spec <t>` as EDN; pin that contract:
        // every target's spec_edn parses as a map whose fields equal the matrix.
        for t in Target::all() {
            let edn = t.spec_edn();
            let m = kami_scene::root_map(&edn)
                .unwrap_or_else(|| panic!("{} spec_edn is not a valid EDN map: {edn}", t.tag()));
            let get = |k: &str| kami_scene::mget(&m, k);
            let s = t.spec();
            assert_eq!(get("target").and_then(|v| v.as_string()), Some(t.tag()));
            assert_eq!(get("jit").and_then(|v| v.as_bool()), Some(s.jit_allowed));
            assert_eq!(get("console-seam").and_then(|v| v.as_bool()), Some(s.console_seam));
            assert_eq!(get("host").and_then(|v| v.as_string()), Some(s.logic.label()));
            match s.host_feature() {
                Some(f) => assert_eq!(get("feature").and_then(|v| v.as_string()), Some(f)),
                None => assert!(get("feature").map_or(false, |v| v.is_nil()), "{} feature nil", t.tag()),
            }
            match t.triple() {
                Some(tr) => assert_eq!(get("triple").and_then(|v| v.as_string()), Some(tr)),
                None => assert!(get("triple").map_or(false, |v| v.is_nil()), "{} triple nil", t.tag()),
            }
        }
    }
}
