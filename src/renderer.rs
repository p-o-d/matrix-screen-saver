use std::sync::{Arc, Mutex};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use crate::{atlas::GlyphAtlas, config::Config, rain::CellState, stats::SystemStats};

/// One depth plane passed to `Renderer::render()`.
pub struct DepthLayer<'a> {
    pub cells: &'a [Vec<CellState>],
    /// Uniform scale applied to cell size and grid spacing (1.0 = nearest/base).
    pub scale: f32,
    /// Multiplied onto each cell's brightness (1.0 = nearest/full).
    pub brightness_mult: f32,
}

/// Per-instance GPU data — one per visible character cell.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Instance {
    pub position: [f32; 2],   // top-left pixel, pre-scaled
    pub atlas_rect: [f32; 4], // UV rect in atlas texture
    pub brightness: f32,
    pub is_head: u32,
    pub scale: f32,           // quad size = cell_size * scale
}

impl Instance {
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
        2 => Float32,
        3 => Uint32,
        4 => Float32,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct RainUniform {
    primary_color: [f32; 4],
    screen_size: [f32; 2],
    cell_size: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct BlurParams {
    direction: [f32; 2],
    intensity: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ScanlineParams {
    intensity: f32,
    width: u32,
    chromatic_aberration: f32,
    _pad: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct RectParams {
    rect: [f32; 4],        // x, y, w, h in pixels
    screen_size: [f32; 2],
    _pad: [f32; 2],        // align color to 32-byte offset
    color: [f32; 4],
}

struct DebugResources {
    atlas: Arc<GlyphAtlas>,
    stats: Arc<Mutex<SystemStats>>,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instance_buf: wgpu::Buffer,
    instance_count: u32,
    rect_pipeline: wgpu::RenderPipeline,
    rect_uniform_buf: wgpu::Buffer,
    rect_bind_group: wgpu::BindGroup,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,

    rain_pipeline: wgpu::RenderPipeline,
    rain_bind_group: wgpu::BindGroup,
    rain_uniform_buf: wgpu::Buffer,

    instance_buf: wgpu::Buffer,
    max_instances: usize,
    instances_scratch: Vec<Instance>,   // reused each frame to avoid per-frame alloc
    debug_instances_scratch: Vec<Instance>,

    // Glow resources
    offscreen_view: wgpu::TextureView,
    blur_h_view: wgpu::TextureView,
    blur_pipeline: wgpu::RenderPipeline,
    blur_h_bind_group: wgpu::BindGroup,

    // Additive blend pipeline (copies offscreen or blends blurred result)
    blend_pipeline: wgpu::RenderPipeline,
    // Bind group for copying offscreen → frame
    copy_bind_group: wgpu::BindGroup,
    // Bind group for blending blur_h (horizontal glow) onto frame
    glow_bind_group: wgpu::BindGroup,

    bg_color: wgpu::Color,
    glow_enabled: bool,
    scanline_pipeline: wgpu::RenderPipeline,
    scanline_bind_group: wgpu::BindGroup,
    scanline_enabled: bool,
    debug: Option<DebugResources>,
    pub width: u32,
    pub height: u32,
    pub atlas: Arc<GlyphAtlas>,
    pub adapter_info: wgpu::AdapterInfo,
    fps: f32,
    last_render: Option<std::time::Instant>,
}

fn build_debug_instances(
    stats: &SystemStats,
    atlas: &GlyphAtlas,
    screen_w: u32,
    _screen_h: u32,
    fps: f32,
    gpu_name: &str,
    out: &mut Vec<Instance>,
) -> [f32; 4] {
    fn bar(pct: f32) -> String {
        let n = ((pct / 100.0 * 10.0).round() as usize).min(10);
        format!("{}{}", "█".repeat(n), "░".repeat(10 - n))
    }
    let ram_pct = if stats.ram_total_gb > 0.0 { stats.ram_used_gb / stats.ram_total_gb * 100.0 } else { 0.0 };
    let vram_pct = if stats.vram_total_gb > 0.0 { stats.vram_used_gb / stats.vram_total_gb * 100.0 } else { 0.0 };
    let lines: [String; 6] = [
        format!(" {}", gpu_name),
        format!(" FPS  {:3.0}", fps),
        format!(" RAM  {}  {:4.1}/{:4.1}GB", bar(ram_pct), stats.ram_used_gb, stats.ram_total_gb),
        format!(" CPU  {}  {:3.0}%", bar(stats.cpu_pct), stats.cpu_pct),
        format!(" VRAM {}  {:4.1}/{:4.1}GB", bar(vram_pct), stats.vram_used_gb, stats.vram_total_gb),
        format!(" GPU  {}  {:3.0}%", bar(stats.gpu_pct), stats.gpu_pct),
    ];
    let cw = atlas.cell_width as f32;
    let ch = atlas.cell_height as f32;
    let max_cols = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let pad = 8.0_f32;
    let panel_w = max_cols as f32 * cw + pad * 2.0;
    let panel_h = lines.len() as f32 * ch + pad * 2.0;
    let margin = 12.0_f32;
    let panel_x = screen_w as f32 - panel_w - margin;
    let panel_y = margin;
    let text_x = panel_x + pad;
    let text_y = panel_y + pad;
    out.clear();
    for (row, line) in lines.iter().enumerate() {
        for (col, glyph) in line.chars().enumerate() {
            let uv = atlas.uv_for_char(glyph);
            out.push(Instance {
                position: [text_x + col as f32 * cw, text_y + row as f32 * ch],
                atlas_rect: uv,
                brightness: 1.0,
                is_head: 0,
                scale: 1.0,
            });
        }
    }
    [panel_x, panel_y, panel_w, panel_h]
}

impl Renderer {
    pub async fn new(
        display_ptr: *mut std::ffi::c_void,
        surface_ptr: *mut std::ffi::c_void,
        width: u32,
        height: u32,
        atlas: Arc<GlyphAtlas>,
        config: &Config,
        debug_atlas: Option<Arc<GlyphAtlas>>,
        stats: Option<Arc<Mutex<SystemStats>>>,
    ) -> Self {
        use raw_window_handle::{
            RawDisplayHandle, RawWindowHandle,
            WaylandDisplayHandle, WaylandWindowHandle,
        };
        use std::ptr::NonNull;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            ..Default::default()
        });

        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: RawDisplayHandle::Wayland(
                    WaylandDisplayHandle::new(NonNull::new(display_ptr).unwrap()),
                ),
                raw_window_handle: RawWindowHandle::Wayland(
                    WaylandWindowHandle::new(NonNull::new(surface_ptr).unwrap()),
                ),
            })
        }.expect("wgpu surface creation failed");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no suitable GPU adapter found");

        let adapter_info = adapter.get_info();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("GPU device request failed");

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter()
            .find(|&&f| f == wgpu::TextureFormat::Bgra8Unorm)
            .copied()
            .unwrap_or(caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let primary_color = Config::parse_color(&config.colors.primary);
        let bg = Config::parse_color(&config.colors.background);
        let bg_color = wgpu::Color {
            r: bg[0] as f64, g: bg[1] as f64, b: bg[2] as f64, a: 1.0,
        };

        // ── Atlas texture ──────────────────────────────────────────────────
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: wgpu::Extent3d {
                width: atlas.atlas_width,
                height: atlas.atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            atlas_tex.as_image_copy(),
            &atlas.data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(atlas.atlas_width),
                rows_per_image: Some(atlas.atlas_height),
            },
            wgpu::Extent3d {
                width: atlas.atlas_width,
                height: atlas.atlas_height,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas_tex.create_view(&Default::default());
        let atlas_sampler_obj = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Rain uniform ───────────────────────────────────────────────────
        let rain_uniform = RainUniform {
            primary_color,
            screen_size: [width as f32, height as f32],
            cell_size: [atlas.cell_width as f32, atlas.cell_height as f32],
        };
        let rain_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rain_uniform"),
            contents: bytemuck::bytes_of(&rain_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Rain bind group layout ─────────────────────────────────────────
        let rain_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rain_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let rain_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rain_bg"),
            layout: &rain_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: rain_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&atlas_sampler_obj) },
            ],
        });

        // ── Rain pipeline ──────────────────────────────────────────────────
        let rain_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/rain.wgsl"));
        let rain_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rain_pl"),
            bind_group_layouts: &[&rain_bgl],
            push_constant_ranges: &[],
        });
        let rain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rain_pipeline"),
            layout: Some(&rain_pl),
            vertex: wgpu::VertexState {
                module: &rain_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[Instance::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &rain_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // ── Offscreen textures for glow ────────────────────────────────────
        let make_offscreen = |label: &'static str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let offscreen_tex = make_offscreen("offscreen");
        let offscreen_view = offscreen_tex.create_view(&Default::default());
        let blur_h_tex = make_offscreen("blur_h");
        let blur_h_view = blur_h_tex.create_view(&Default::default());

        let blur_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Blur bind group layout ─────────────────────────────────────────
        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    }, count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    }, count: None,
                },
            ],
        });

        // Horizontal blur: offscreen → blur_h
        let blur_h_params = BlurParams {
            direction: [1.0, 0.0],
            intensity: config.colors.glow_intensity,
            _pad: 0.0,
        };
        let blur_h_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur_h_uniform"),
            contents: bytemuck::bytes_of(&blur_h_params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let blur_h_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur_h_bg"), layout: &blur_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&offscreen_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blur_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: blur_h_uniform_buf.as_entire_binding() },
            ],
        });

        // ── Blur pipeline (full-screen triangle) ───────────────────────────
        let blur_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/blur.wgsl"));
        let blur_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur_pl"), bind_group_layouts: &[&blur_bgl], push_constant_ranges: &[],
        });
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur_pipeline"), layout: Some(&blur_pl),
            vertex: wgpu::VertexState {
                module: &blur_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format, blend: None, write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // ── Blend / copy pipeline ──────────────────────────────────────────
        // Uses the same blur shader (full-screen triangle) but with additive
        // blending so we can composite the glow onto the frame.
        let blend_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blend_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    }, count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false,
                        min_binding_size: None,
                    }, count: None,
                },
            ],
        });

        let passthrough_params = BlurParams { direction: [0.0, 0.0], intensity: 1.0, _pad: 0.0 };
        let passthrough_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("passthrough_uniform"),
            contents: bytemuck::bytes_of(&passthrough_params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // copy_bind_group: sample offscreen → write to frame (no blend)
        let copy_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("copy_bg"), layout: &blend_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&offscreen_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blur_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: passthrough_uniform.as_entire_binding() },
            ],
        });
        // glow_bind_group: sample blur_h (horizontal glow) → additive blend onto frame
        let glow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glow_bg"), layout: &blend_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&blur_h_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blur_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: passthrough_uniform.as_entire_binding() },
            ],
        });

        let blend_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blend_pl"), bind_group_layouts: &[&blend_bgl], push_constant_ranges: &[],
        });
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let blend_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blend_pipeline"), layout: Some(&blend_pl),
            vertex: wgpu::VertexState {
                module: &blur_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format, blend: Some(additive), write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // ── Scanline pipeline ──────────────────────────────────────────────
        let scanline_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/scanline.wgsl"));
        let scanline_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scanline_uniform"),
            contents: bytemuck::bytes_of(&ScanlineParams {
                intensity: config.display.scanline_intensity,
                width: ((config.display.font_size / 18.0).round() as u32).max(1),
                chromatic_aberration: config.display.chromatic_aberration,
                _pad: 0,
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        // Reuse blend_bgl layout (texture + sampler + uniform) — same structure.
        let scanline_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scanline_bg"), layout: &blend_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&offscreen_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blur_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: scanline_uniform_buf.as_entire_binding() },
            ],
        });
        let scanline_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scanline_pl"), bind_group_layouts: &[&blend_bgl], push_constant_ranges: &[],
        });
        let scanline_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scanline_pipeline"), layout: Some(&scanline_pl),
            vertex: wgpu::VertexState {
                module: &scanline_shader, entry_point: "vs_main",
                compilation_options: Default::default(), buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &scanline_shader, entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format, blend: None, write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // ── Instance buffer ────────────────────────────────────────────────
        // Sum cell counts across all depth levels (far level has smallest scale → most cells).
        let base_cw = atlas.cell_width as f32;
        let base_ch = atlas.cell_height as f32;
        let n = (config.rain.depth_levels as usize).max(1);
        let min_s = config.rain.depth_scale_min.max(0.1);
        let max_instances: usize = (0..n).map(|i| {
            let t = if n == 1 { 1.0_f32 } else { i as f32 / (n - 1) as f32 };
            let s = min_s + (1.0 - min_s) * t;
            let cols = (width as f32 / (base_cw * s)).ceil() as usize;
            let rows = (height as f32 / (base_ch * s)).ceil() as usize;
            cols * rows
        }).sum::<usize>() + 256;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buf"),
            size: (max_instances * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Debug overlay resources ────────────────────────────────────────
        let debug = if let (Some(dbg_atlas), Some(dbg_stats)) = (debug_atlas, stats) {
            let primary_color = Config::parse_color(&config.colors.primary);

            // Upload debug atlas texture (R8Unorm, same format as rain atlas)
            let dbg_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("debug_atlas"),
                size: wgpu::Extent3d {
                    width: dbg_atlas.atlas_width, height: dbg_atlas.atlas_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                dbg_tex.as_image_copy(),
                &dbg_atlas.data,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(dbg_atlas.atlas_width),
                    rows_per_image: Some(dbg_atlas.atlas_height),
                },
                wgpu::Extent3d {
                    width: dbg_atlas.atlas_width, height: dbg_atlas.atlas_height,
                    depth_or_array_layers: 1,
                },
            );
            let dbg_tex_view = dbg_tex.create_view(&Default::default());
            let dbg_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });

            // Debug uniform buffer — RainUniform with debug cell dimensions
            let dbg_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("debug_uniform"),
                contents: bytemuck::bytes_of(&RainUniform {
                    primary_color,
                    screen_size: [width as f32, height as f32],
                    cell_size: [dbg_atlas.cell_width as f32, dbg_atlas.cell_height as f32],
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            // Debug bind group: same layout as rain_bgl (uniform + texture + sampler)
            let dbg_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("debug_bg"), layout: &rain_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: dbg_uniform_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&dbg_tex_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&dbg_sampler) },
                ],
            });

            // Debug instance buffer — fixed 512 slots (~14 KB)
            let dbg_instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("debug_instance_buf"),
                size: (512 * std::mem::size_of::<Instance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Rect pipeline — solid colored background quad, alpha blended
            let rect_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/rect.wgsl"));
            let rect_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rect_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: None,
                    }, count: None,
                }],
            });
            let rect_uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rect_uniform"),
                size: std::mem::size_of::<RectParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let rect_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rect_bg"), layout: &rect_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0, resource: rect_uniform_buf.as_entire_binding(),
                }],
            });
            let rect_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("rect_pl"), bind_group_layouts: &[&rect_bgl], push_constant_ranges: &[],
            });
            let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("rect_pipeline"), layout: Some(&rect_pl),
                vertex: wgpu::VertexState {
                    module: &rect_shader, entry_point: "vs_main",
                    compilation_options: Default::default(), buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &rect_shader, entry_point: "fs_main",
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format, blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            });

            Some(DebugResources {
                atlas: dbg_atlas,
                stats: dbg_stats,
                uniform_buf: dbg_uniform_buf,
                bind_group: dbg_bind_group,
                instance_buf: dbg_instance_buf,
                instance_count: 0,
                rect_pipeline,
                rect_uniform_buf,
                rect_bind_group,
            })
        } else {
            None
        };

        Self {
            device, queue, surface, surface_config,
            rain_pipeline, rain_bind_group, rain_uniform_buf,
            instance_buf, max_instances,
            offscreen_view, blur_h_view,
            blur_pipeline, blur_h_bind_group,
            blend_pipeline, copy_bind_group, glow_bind_group,
            bg_color, glow_enabled: config.colors.glow,
            scanline_pipeline, scanline_bind_group,
            scanline_enabled: config.display.scanlines || config.display.chromatic_aberration > 0.0,
            debug,
            width, height, atlas,
            adapter_info,
            fps: 0.0,
            last_render: None,
            instances_scratch: Vec::with_capacity(max_instances),
            debug_instances_scratch: Vec::with_capacity(512),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.width = width;
        self.height = height;
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Render one frame. `layers` are ordered far→near; near instances are drawn on top.
    pub fn render(&mut self, layers: &[DepthLayer<'_>]) {
        let now = std::time::Instant::now();
        if let Some(prev) = self.last_render {
            let dt = now.duration_since(prev).as_secs_f32();
            if dt > 0.0 {
                let sample = 1.0 / dt;
                self.fps = if self.fps == 0.0 { sample } else { self.fps * 0.9 + sample * 0.1 };
            }
        }
        self.last_render = Some(now);

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("surface error: {e}");
                return;
            }
        };
        let frame_view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        let cw = self.atlas.cell_width as f32;
        let ch = self.atlas.cell_height as f32;

        // Build instance buffer: all depth layers far→near (painter's algorithm).
        self.instances_scratch.clear();
        for layer in layers {
            let lcw = cw * layer.scale;
            let lch = ch * layer.scale;
            for (row_idx, row) in layer.cells.iter().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    if cell.brightness < 0.01 { continue; }
                    if self.instances_scratch.len() == self.max_instances { break; }
                    let uv = self.atlas.uv_for_char(cell.ch);
                    self.instances_scratch.push(Instance {
                        position: [col_idx as f32 * lcw, row_idx as f32 * lch],
                        atlas_rect: uv,
                        brightness: (cell.brightness * layer.brightness_mult).min(1.0),
                        is_head: cell.is_head as u32,
                        scale: layer.scale,
                    });
                }
            }
        }

        if !self.instances_scratch.is_empty() {
            self.queue.write_buffer(
                &self.instance_buf,
                0,
                bytemuck::cast_slice(&self.instances_scratch),
            );
        }

        // Update debug overlay buffers
        let dbg_count: u32 = if let Some(ref mut debug) = self.debug {
            if let Ok(stats) = debug.stats.try_lock() {
                let rect = build_debug_instances(
                    &stats, &debug.atlas, self.width, self.height, self.fps,
                    &self.adapter_info.name,
                    &mut self.debug_instances_scratch,
                );
                let count = (self.debug_instances_scratch.len().min(512)) as u32;
                if count > 0 {
                    let rect_params = RectParams {
                        rect,
                        screen_size: [self.width as f32, self.height as f32],
                        _pad: [0.0; 2],
                        color: [0.0, 0.0, 0.0, 0.75],
                    };
                    self.queue.write_buffer(&debug.rect_uniform_buf, 0, bytemuck::bytes_of(&rect_params));
                    self.queue.write_buffer(&debug.instance_buf, 0, bytemuck::cast_slice(&self.debug_instances_scratch[..count as usize]));
                }
                debug.instance_count = count;
                count
            } else {
                debug.instance_count
            }
        } else {
            0
        };

        let clear_bg = wgpu::Operations {
            load: wgpu::LoadOp::Clear(self.bg_color),
            store: wgpu::StoreOp::Store,
        };

        if self.glow_enabled {
            // Pass 1: render rain into the offscreen texture
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("rain_offscreen"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.offscreen_view,
                        resolve_target: None,
                        ops: clear_bg,
                    })],
                    ..Default::default()
                });
                if !self.instances_scratch.is_empty() {
                    pass.set_pipeline(&self.rain_pipeline);
                    pass.set_bind_group(0, &self.rain_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buf.slice(..));
                    pass.draw(0..6, 0..self.instances_scratch.len() as u32);
                }
                if dbg_count > 0 {
                    if let Some(debug) = &self.debug {
                        pass.set_pipeline(&debug.rect_pipeline);
                        pass.set_bind_group(0, &debug.rect_bind_group, &[]);
                        pass.draw(0..6, 0..1);
                        pass.set_pipeline(&self.rain_pipeline);
                        pass.set_bind_group(0, &debug.bind_group, &[]);
                        pass.set_vertex_buffer(0, debug.instance_buf.slice(..));
                        pass.draw(0..6, 0..dbg_count);
                    }
                }
            }

            // Pass 2: horizontal blur offscreen → blur_h
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blur_h"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.blur_h_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.blur_pipeline);
                pass.set_bind_group(0, &self.blur_h_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            // Pass 3: copy offscreen to frame (with scanlines if enabled)
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("copy_to_frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                if self.scanline_enabled {
                    pass.set_pipeline(&self.scanline_pipeline);
                    pass.set_bind_group(0, &self.scanline_bind_group, &[]);
                } else {
                    pass.set_pipeline(&self.blend_pipeline);
                    pass.set_bind_group(0, &self.copy_bind_group, &[]);
                }
                pass.draw(0..3, 0..1);
            }

            // Pass 4: additive blend blur_h (horizontal glow) onto frame
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("glow_blend"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.blend_pipeline);
                pass.set_bind_group(0, &self.glow_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        } else if self.scanline_enabled {
            // No glow, but scanlines: rain → offscreen, then scanline → frame
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("rain_offscreen"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.offscreen_view,
                        resolve_target: None,
                        ops: clear_bg,
                    })],
                    ..Default::default()
                });
                if !self.instances_scratch.is_empty() {
                    pass.set_pipeline(&self.rain_pipeline);
                    pass.set_bind_group(0, &self.rain_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buf.slice(..));
                    pass.draw(0..6, 0..self.instances_scratch.len() as u32);
                }
                if dbg_count > 0 {
                    if let Some(debug) = &self.debug {
                        pass.set_pipeline(&debug.rect_pipeline);
                        pass.set_bind_group(0, &debug.rect_bind_group, &[]);
                        pass.draw(0..6, 0..1);
                        pass.set_pipeline(&self.rain_pipeline);
                        pass.set_bind_group(0, &debug.bind_group, &[]);
                        pass.set_vertex_buffer(0, debug.instance_buf.slice(..));
                        pass.draw(0..6, 0..dbg_count);
                    }
                }
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("scanline_to_frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.scanline_pipeline);
                pass.set_bind_group(0, &self.scanline_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        } else {
            // No glow, no scanlines: render directly to frame
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rain_direct"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame_view,
                    resolve_target: None,
                    ops: clear_bg,
                })],
                ..Default::default()
            });
            if !self.instances_scratch.is_empty() {
                pass.set_pipeline(&self.rain_pipeline);
                pass.set_bind_group(0, &self.rain_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buf.slice(..));
                pass.draw(0..6, 0..self.instances_scratch.len() as u32);
            }
            if dbg_count > 0 {
                if let Some(debug) = &self.debug {
                    pass.set_pipeline(&debug.rect_pipeline);
                    pass.set_bind_group(0, &debug.rect_bind_group, &[]);
                    pass.draw(0..6, 0..1);
                    pass.set_pipeline(&self.rain_pipeline);
                    pass.set_bind_group(0, &debug.bind_group, &[]);
                    pass.set_vertex_buffer(0, debug.instance_buf.slice(..));
                    pass.draw(0..6, 0..dbg_count);
                }
            }
        }

        self.queue.submit([encoder.finish()]);
        frame.present();
    }
}
