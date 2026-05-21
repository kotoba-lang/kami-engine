//! KAMI Engine boot logo + splash screen.
//!
//! SVG torii gate + "KAMI ENGINE" text, rendered as GPU quad with fade-in animation.

/// KAMI Engine logo as inline SVG string.
pub const LOGO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 200">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#f59e0b"/>
      <stop offset="100%" stop-color="#d97706"/>
    </linearGradient>
  </defs>
  <!-- Torii gate -->
  <rect x="120" y="40" width="160" height="12" rx="6" fill="url(#g)"/>
  <rect x="130" y="52" width="140" height="8" rx="4" fill="url(#g)"/>
  <rect x="140" y="60" width="12" height="80" fill="url(#g)"/>
  <rect x="248" y="60" width="12" height="80" fill="url(#g)"/>
  <rect x="132" y="90" width="136" height="8" rx="4" fill="url(#g)"/>
  <!-- Text -->
  <text x="200" y="170" text-anchor="middle" font-family="system-ui,sans-serif" font-size="28" font-weight="700" fill="#f59e0b" letter-spacing="8">KAMI ENGINE</text>
  <text x="200" y="190" text-anchor="middle" font-family="system-ui,sans-serif" font-size="10" fill="#888" letter-spacing="4">NEXT-GEN GAME PLATFORM</text>
</svg>"##;

/// Splash screen state.
pub struct SplashScreen {
    elapsed: f32,
    duration: f32,
    fade_in: f32,
    fade_out: f32,
}

impl SplashScreen {
    pub fn new() -> Self {
        Self {
            elapsed: 0.0,
            duration: 2.0,
            fade_in: 0.5,
            fade_out: 0.3,
        }
    }

    /// Advance splash timer. Returns false when splash is done.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        self.elapsed < self.duration
    }

    /// Current opacity (0.0 → 1.0 → 0.0).
    pub fn opacity(&self) -> f32 {
        if self.elapsed < self.fade_in {
            self.elapsed / self.fade_in
        } else if self.elapsed > self.duration - self.fade_out {
            (self.duration - self.elapsed) / self.fade_out
        } else {
            1.0
        }
    }

    /// Progress bar (0.0 → 1.0).
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    pub fn is_done(&self) -> bool {
        self.elapsed >= self.duration
    }
}

/// Logo as WGSL-compatible clear color (amber brand color).
pub const BRAND_COLOR: [f32; 4] = [0.961, 0.620, 0.043, 1.0]; // #f59e0b

/// Background color for splash screen.
pub const SPLASH_BG: [f32; 4] = [0.05, 0.05, 0.07, 1.0];

impl Default for SplashScreen {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_lifecycle() {
        let mut splash = SplashScreen::new();
        assert!(!splash.is_done());
        assert!(splash.opacity() < 0.1); // start faded

        // Fade in
        splash.tick(0.5);
        assert!((splash.opacity() - 1.0).abs() < 0.01);

        // Hold
        splash.tick(1.0);
        assert!((splash.opacity() - 1.0).abs() < 0.01);

        // Fade out
        splash.tick(0.4);
        assert!(splash.opacity() < 1.0);

        // Done
        splash.tick(0.2);
        assert!(splash.is_done());
    }

    #[test]
    fn logo_svg_valid() {
        assert!(LOGO_SVG.contains("KAMI ENGINE"));
        assert!(LOGO_SVG.contains("<svg"));
    }
}
