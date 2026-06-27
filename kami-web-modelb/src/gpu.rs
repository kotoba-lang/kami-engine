//! Browser GPU display for the web Model-B dance (compliant: wgpu only, never
//! Canvas2D). Step 1 — bring up a wgpu surface from the page `<canvas>` (reusing
//! `kami_render::RenderContext::for_web_surface`, the same path kami-web uses) and
//! clear/present it. The render-IR → pixels blit layers on top of this.
//!
//! wasm32-only (a `<canvas>` + `SurfaceTarget::Canvas` exist only in the browser).

use kami_render::RenderContext;
use wasm_bindgen::JsCast;

/// A bound wgpu surface over the page canvas.
pub struct Gpu {
    ctx: RenderContext,
}

impl Gpu {
    /// Acquire the `<canvas id=…>` and bring up a wgpu surface over it.
    pub async fn new(canvas_id: &str, w: u32, h: u32) -> Result<Gpu, String> {
        let canvas: web_sys::HtmlCanvasElement = web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.get_element_by_id(canvas_id))
            .ok_or_else(|| format!("canvas #{canvas_id} not found"))?
            .dyn_into()
            .map_err(|_| "element is not a <canvas>".to_string())?;
        canvas.set_width(w);
        canvas.set_height(h);
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let ctx = RenderContext::for_web_surface(target, w, h, "kami-web-modelb")
            .await
            .map_err(|e| format!("wgpu surface: {e:?}"))?;
        Ok(Gpu { ctx })
    }

    /// Clear the surface to the Nintendo cream stage colour and present. (Step 1
    /// proof the wgpu path is live; the render-IR blit replaces the clear next.)
    pub fn present_clear(&self) -> Result<(), String> {
        let frame = self
            .ctx
            .surface
            .get_current_texture()
            .map_err(|e| format!("acquire frame: {e:?}"))?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let _rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.94, g: 0.92, b: 0.84, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.ctx.queue.submit([enc.finish()]);
        frame.present();
        Ok(())
    }
}
