//! platform — the ADR-0037 packaging matrix as executable data.
//!
//! The cross-platform decisions (does this target ban JIT? which WASM host,
//! texture format, renderer backend, and default input does it use?) were prose
//! in ADR-0037; this module makes them a single source of truth the packaging
//! tooling (`bb kge host/package`) and CI consume. Pure data + pure functions,
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

    /// Lowercase tag used on the `bb kge --target <tag>` CLI and in `:platform`.
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
}
