//! E2 build 2 — build 1 (parley + `vello_cpu`) plus `vello`/`wgpu`, both
//! backends reachable from `main`. This is §12.2's headline size lever
//! (§12.2: "drop `wgpu`/`vello`, ship `tiny-skia` only: −4–6 MB", C10: that
//! figure has no source) — this crate exists to price it against a real
//! `dist`-profile build rather than an anecdote.
//!
//! ## The DCE guard
//!
//! Dispatch is on `std::env::args()`, which `opt-level = "s"` + `lto = "fat"`
//! cannot evaluate at compile time, so neither branch is provably dead. The
//! `gpu` branch does real work — builds a `vello::Scene` with real glyphs,
//! initializes `wgpu`, renders to a texture, reads the pixels back, hashes
//! and prints them — and consumes the result rather than dropping it.
//!
//! wgpu adapter/device creation MAY fail on this machine (headless CI, no
//! GPU, driver issue). That is fine and does not affect the size
//! measurement: linkage is what is being measured, and the code has to be
//! reachable either way, so it is never `#[cfg]`d out. A failure here is
//! reported, not swallowed, and does not change which bytes the linker kept.
use e2_common::parley::{self, PositionedLayoutItem};
use vello::wgpu; // re-exported by vello, so this crate does not pin its own wgpu version

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    if arg == "gpu" {
        pollster::block_on(run_gpu());
    } else {
        e2_common::run_and_print_baseline("vello_cpu+wgpu");
    }
}

async fn run_gpu() {
    let (mut font_cx, family) = e2_common::font_context();
    let mut layout_cx: parley::LayoutContext<()> = parley::LayoutContext::new();
    let layout = e2_common::build_layout(
        &mut font_cx,
        &mut layout_cx,
        &family,
        &format!("{}gpu", e2_common::SAMPLE_TEXT),
        16.0,
    );

    let mut scene = vello::Scene::new();
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(gr) = item {
                let run = gr.run();
                let font = run.font();
                let size = run.font_size();
                let glyphs: Vec<vello::Glyph> = gr
                    .positioned_glyphs()
                    .map(|g| vello::Glyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                    })
                    .collect();
                scene
                    .draw_glyphs(font)
                    .font_size(size)
                    .hint(true)
                    .brush(vello::peniko::Color::BLACK)
                    .draw(vello::peniko::Fill::NonZero, glyphs.into_iter());
            }
        }
    }

    let w = (layout.width().ceil().max(1.0)) as u32;
    let h = (layout.height().ceil().max(1.0)) as u32;

    match render_to_bytes(&scene, w, h).await {
        Ok(bytes) => {
            let hash = e2_common::fnv1a(&bytes);
            println!(
                "[vello/wgpu] GPU render OK: {w}x{h} bytes={} hash={hash:016x}",
                bytes.len()
            );
        }
        Err(e) => {
            // Linkage still happened — every type and function above this
            // point in `run_gpu` was compiled and is reachable; only the
            // runtime device/adapter step failed. See module docs.
            println!("[vello/wgpu] GPU render FAILED at runtime (linkage unaffected): {e}");
        }
    }
}

async fn render_to_bytes(scene: &vello::Scene, w: u32, h: u32) -> Result<Vec<u8>, String> {
    let mut rc = vello::util::RenderContext::new();
    let dev_id = rc
        .device(None)
        .await
        .ok_or("no compatible wgpu adapter/device found")?;
    let device_handle = &rc.devices[dev_id];
    let device = &device_handle.device;
    let queue = &device_handle.queue;

    let mut renderer = vello::Renderer::new(
        device,
        vello::RendererOptions {
            use_cpu: false,
            antialiasing_support: vello::AaSupport::area_only(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .map_err(|e| format!("Renderer::new: {e}"))?;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("e2-vello-gpu target"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let params = vello::RenderParams {
        base_color: vello::peniko::Color::WHITE,
        width: w,
        height: h,
        antialiasing_method: vello::AaConfig::Area,
    };
    renderer
        .render_to_texture(device, queue, scene, &view, &params)
        .map_err(|e| format!("render_to_texture: {e}"))?;

    let bytes_per_row = (w * 4).next_multiple_of(256);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("e2-vello-gpu readback"),
        size: (bytes_per_row as u64) * (h as u64),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("device.poll: {e}"))?;
    rx.recv()
        .map_err(|e| format!("map_async channel: {e}"))?
        .map_err(|e| format!("map_async: {e:?}"))?;
    let data = slice.get_mapped_range().to_vec();
    Ok(data)
}
