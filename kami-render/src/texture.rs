//! GPU texture creation, mipmap generation, and fallback textures.

#[cfg(feature = "wgpu-backend")]
use wgpu::util::DeviceExt;

/// GPU texture with view and sampler.
#[cfg(feature = "wgpu-backend")]
pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
}

/// Create a GPU texture from RGBA8 data with optional mipmaps.
#[cfg(feature = "wgpu-backend")]
pub fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &[u8],
    width: u32,
    height: u32,
    label: &str,
    generate_mipmaps: bool,
) -> GpuTexture {
    let mip_levels = if generate_mipmaps {
        (width.max(height) as f32).log2().floor() as u32 + 1
    } else {
        1
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | if generate_mipmaps {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    if generate_mipmaps && mip_levels > 1 {
        generate_mipmaps_cpu(queue, &texture, width, height, mip_levels, data);
    }

    let view = texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(&format!("{}-sampler", label)),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    GpuTexture {
        texture,
        view,
        sampler,
        width,
        height,
        mip_levels,
    }
}

/// CPU-side mipmap generation (box filter downsample).
#[cfg(feature = "wgpu-backend")]
fn generate_mipmaps_cpu(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    base_width: u32,
    base_height: u32,
    mip_levels: u32,
    base_data: &[u8],
) {
    let mut prev_data = base_data.to_vec();
    let mut w = base_width;
    let mut h = base_height;

    for level in 1..mip_levels {
        let new_w = (w / 2).max(1);
        let new_h = (h / 2).max(1);
        let mut new_data = vec![0u8; (new_w * new_h * 4) as usize];

        for y in 0..new_h {
            for x in 0..new_w {
                let sx = (x * 2) as usize;
                let sy = (y * 2) as usize;
                let stride = w as usize * 4;

                for c in 0..4 {
                    let mut sum = 0u32;
                    let mut count = 0u32;
                    for dy in 0..2u32 {
                        for dx in 0..2u32 {
                            let px = (sx + dx as usize).min(w as usize - 1);
                            let py = (sy + dy as usize).min(h as usize - 1);
                            sum += prev_data[py * stride + px * 4 + c] as u32;
                            count += 1;
                        }
                    }
                    new_data[(y * new_w + x) as usize * 4 + c] = (sum / count) as u8;
                }
            }
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &new_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * new_w),
                rows_per_image: Some(new_h),
            },
            wgpu::Extent3d {
                width: new_w,
                height: new_h,
                depth_or_array_layers: 1,
            },
        );

        prev_data = new_data;
        w = new_w;
        h = new_h;
    }
}

/// 1x1 white pixel texture (fallback for untextured albedo).
#[cfg(feature = "wgpu-backend")]
pub fn default_white_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuTexture {
    create_texture(
        device,
        queue,
        &[255, 255, 255, 255],
        1,
        1,
        "default-white",
        false,
    )
}

/// 1x1 flat normal map (0.5, 0.5, 1.0 = up in tangent space).
#[cfg(feature = "wgpu-backend")]
pub fn default_normal_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuTexture {
    create_texture(
        device,
        queue,
        &[128, 128, 255, 255],
        1,
        1,
        "default-normal",
        false,
    )
}

/// 1x1 default metallic-roughness (metallic=0, roughness=0.5).
/// glTF convention: G=roughness, B=metallic.
#[cfg(feature = "wgpu-backend")]
pub fn default_mr_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuTexture {
    create_texture(device, queue, &[0, 128, 0, 255], 1, 1, "default-mr", false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn mip_level_calculation() {
        // 1024x1024 → log2(1024) + 1 = 11
        let mips = (1024u32.max(1024) as f32).log2().floor() as u32 + 1;
        assert_eq!(mips, 11);

        // 4x4 → log2(4) + 1 = 3
        let mips = (4u32.max(4) as f32).log2().floor() as u32 + 1;
        assert_eq!(mips, 3);

        // 1x1 → log2(1) + 1 = 1
        let mips = (1u32.max(1) as f32).log2().floor() as u32 + 1;
        assert_eq!(mips, 1);
    }
}
