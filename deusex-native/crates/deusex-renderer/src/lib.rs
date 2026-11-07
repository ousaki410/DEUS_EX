//! deusex-renderer - wgpu-based rendering backend.
//!
//! This crate owns the GPU device and the swapchain surface. It distributes no
//! game assets; all textures, meshes and shaders are produced from the user's
//! legally owned installation at runtime.
//!
//! Surface creation itself is intentionally left to the caller (e.g. the SDL3
//! window integration), which already has the native window handle. This crate
//! configures the surface, owns the render loop primitives, and exposes the
//! device/queue for resource creation.
//!
//! Current state: device/queue/adapter initialization plus a sprite rendering
//! pipeline. Call `begin_frame()`, then one or more `draw_sprite()`, then
//! `render()` to present a frame (double-buffered via the wgpu swapchain).

use std::collections::HashMap;

use tracing::info;
use wgpu::{Adapter, Device, Extent3d, Instance, Queue, Surface};
use wgpu::util::DeviceExt;

/// A live wgpu device plus its queue.
#[derive(Debug)]
pub struct Gpu {
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
}

impl Gpu {
    /// Initialize a wgpu instance, adapter, device and queue without a
    /// window/surface. Useful for headless testing and for the renderer
    /// skeleton before SDL3 integration.
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or("no wgpu adapter found")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: Some("deusex-renderer device"),
                },
                None,
            )
            .await?;

        info!(
            "renderer: initialized wgpu device ({:?}, backend {:?})",
            adapter.get_info(),
            adapter.get_info().backend
        );

        Ok(Gpu {
            instance,
            adapter,
            device,
            queue,
        })
    }
}

/// 纹理资源句柄，由 `Renderer` 管理的 GPU 纹理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(usize);

impl TextureId {
    pub fn null() -> Self {
        Self(usize::MAX)
    }
    pub fn is_null(&self) -> bool {
        self.0 == usize::MAX
    }
}

/// 纹理上传描述：原始 RGBA8 像素数据。
#[derive(Debug, Clone)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    /// 预期 `width * height * 4` 字节，RGBA8 顺序。
    pub data: Vec<u8>,
}

/// 渲染一个 sprite 的描述（屏幕像素空间，Y 向下）。
#[derive(Debug, Clone, Copy)]
pub struct SpriteDraw {
    /// 纹理句柄（`TextureId::null()` 表示绘制纯色矩形）。
    pub texture: TextureId,
    /// 目标矩形（屏幕像素坐标，左上角原点）。
    pub rect: SpriteRect,
    /// 与纹理采样值相乘的颜色（`wgpu::Color::WHITE` = 不染色）。
    pub color: wgpu::Color,
}

/// 渲染目标矩形，使用 f32 像素坐标。
#[derive(Debug, Clone, Copy)]
pub struct SpriteRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// 顶点布局：屏幕空间位置 + UV + 颜色。
#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
}

/// 顶点属性描述（静态，避免临时值生命周期问题）。
const VERTEX_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];

/// 顶点缓冲区布局描述。
fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERTEX_ATTRS,
    }
}

/// 将顶点数组转换为字节切片（用于 upload）。
fn vertices_to_bytes(vertices: &[Vertex]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            vertices.len() * std::mem::size_of::<Vertex>(),
        )
    }
}

/// 将索引数组转换为字节切片。
fn indices_to_bytes(indices: &[u16]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            indices.as_ptr() as *const u8,
            indices.len() * std::mem::size_of::<u16>(),
        )
    }
}

/// 着色器源码：顶点着色器将屏幕坐标映射到 NDC，片元着色器采样纹理并调制颜色。
const VS_SRC: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(position.xy, 0.0, 1.0);
    output.uv = uv;
    output.color = color;
    return output;
}
"#;

const FS_SRC: &str = r#"
@group(0) @binding(0) var texture_sampler: sampler;
@group(0) @binding(1) var texture_view: texture_2d<f32>;

@fragment
fn fs_main(
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
) -> @location(0) vec4<f32> {
    var tex_color = textureSample(texture_view, texture_sampler, uv);
    return tex_color * color;
}
"#;

/// 已上传到 GPU 的纹理资源（字段不公开，通过 `TextureId` 索引访问）。
///
/// `wgpu::TextureView` 和 `wgpu::Sampler` 不支持 `Clone`，因此在每次
/// 绘制时从 `texture` 重新创建。
#[derive(Debug)]
struct Texture {
    id: TextureId,
    _texture: wgpu::Texture,
    width: u32,
    height: u32,
}

/// Surface-bound rendering context.
///
/// Created once a native window (SDL3) exists and the caller has already
/// produced a `wgpu::Surface` for it (via `instance.create_surface(...)`).
#[derive(Debug)]
pub struct Renderer {
    pub gpu: Gpu,
    pub surface: Surface<'static>,
    /// Logical size of the surface in pixels.
    pub size: Extent3d,
    /// Current swapchain format.
    pub surface_format: wgpu::TextureFormat,
    /// Uploaded textures keyed by `TextureId`.
    textures: HashMap<TextureId, Texture>,
    next_texture_id: usize,
    /// wgpu render pipeline for sprite drawing.
    pipeline: wgpu::RenderPipeline,
    /// Bind group layout shared by all sprite draws.
    bind_group_layout: wgpu::BindGroupLayout,
    /// Vertex buffer (dynamic, re-uploaded each frame).
    vertex_buffer: wgpu::Buffer,
    /// Index buffer (shared quad index pattern).
    index_buffer: wgpu::Buffer,
    index_count: u32,
    /// Per-frame bind group (recreated each draw call).
    current_bind_group: Option<wgpu::BindGroup>,
}

impl Renderer {
    /// Create a renderer bound to an existing surface.
    ///
    /// `surface` must outlive the renderer (the caller keeps the window alive).
    pub fn new(
        gpu: Gpu,
        surface: Surface<'static>,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let size = Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &config);

        let device = &gpu.device;

        // Bind group layout: sampler + texture view.
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("deusex-renderer sprite bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
                ],
            });

        // Compile shaders.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("deusex-renderer sprite fragment shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(FS_SRC)),
        });
        let vs_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("deusex-renderer sprite vertex shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(VS_SRC)),
        });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("deusex-renderer sprite pipeline layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("deusex-renderer sprite pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_shader,
                entry_point: "vs_main",
                buffers: &[vertex_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Shared quad index buffer: two triangles forming a unit square.
        let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("deusex-renderer quad index buffer"),
            contents: indices_to_bytes(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Dynamic vertex buffer: enough for 1024 sprites per frame initially.
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("deusex-renderer dynamic vertex buffer"),
            size: (std::mem::size_of::<Vertex>() * 4 * 1024) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        info!(
            "renderer: configured surface {}x{} format {:?}",
            size.width, size.height, format
        );

        Ok(Renderer {
            gpu,
            surface,
            size,
            surface_format: format,
            textures: HashMap::new(),
            next_texture_id: 0,
            pipeline,
            bind_group_layout,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            current_bind_group: None,
        })
    }

    /// Upload RGBA8 pixel data and return a `TextureId` handle.
    pub fn upload_texture(
        &mut self,
        data: TextureData,
    ) -> Result<TextureId, Box<dyn std::error::Error>> {
        let device = &self.gpu.device;
        let queue = &self.gpu.queue;

        let size = wgpu::Extent3d {
            width: data.width,
            height: data.height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("deusex-renderer uploaded texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data.data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(data.width * 4),
                rows_per_image: Some(data.height),
            },
            size,
        );

        let id = TextureId(self.next_texture_id);
        self.next_texture_id += 1;
        self.textures.insert(
            id,
            Texture {
                id,
                _texture: texture,
                width: data.width,
                height: data.height,
            },
        );

        info!("renderer: uploaded texture {} ({}x{})", id.0, data.width, data.height);
        Ok(id)
    }

    /// Reconfigure the surface after a resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            width: self.size.width,
            height: self.size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        self.surface.configure(&self.gpu.device, &config);
        info!("renderer: resized surface {}x{}", width, height);
    }

    /// Begin a new frame: clear previous per-frame state.
    pub fn begin_frame(&mut self) {
        self.current_bind_group = None;
    }

    /// Draw one sprite.
    ///
    /// `rect` is in screen pixels with the top-left as origin. `color` is
    /// multiplied with the sampled texel (use `wgpu::Color::WHITE` for no
    /// tinting). A `null` texture draws a solid-color rectangle.
    pub fn draw_sprite(
        &mut self,
        sprite: SpriteDraw,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let device = &self.gpu.device;
        let queue = &self.gpu.queue;

        // Look up the texture (or use a 1x1 transparent fallback).
        let (sampler, view) = if sprite.texture.is_null() {
            let fb = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("deusex-renderer fallback texture"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
            let view = fb.create_view(&wgpu::TextureViewDescriptor::default());
            (sampler, view)
        } else {
            let tex = self.textures.get(&sprite.texture).ok_or("texture not found")?;
            let view = tex
                ._texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("deusex-renderer draw sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
            (sampler, view)
        };

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("deusex-renderer per-frame bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });

        self.current_bind_group = Some(bind_group);

        // Compute vertex positions in NDC (-1..1), Y-down screen space.
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let x0 = sprite.rect.x;
        let y0 = sprite.rect.y;
        let x1 = x0 + sprite.rect.w;
        let y1 = y0 + sprite.rect.h;

        // Convert screen pixels to NDC: x in [-1,1], y in [-1,1] (Y down).
        let ndc = |px: f32, py: f32| -> [f32; 3] {
            [(px / w) * 2.0 - 1.0, 1.0 - (py / h) * 2.0, 0.0]
        };

        let color_arr = [
            sprite.color.r as f32,
            sprite.color.g as f32,
            sprite.color.b as f32,
            sprite.color.a as f32,
        ];

        let vertices: [Vertex; 4] = [
            Vertex {
                position: ndc(x0, y0),
                uv: [0.0, 0.0],
                color: color_arr,
            },
            Vertex {
                position: ndc(x1, y0),
                uv: [1.0, 0.0],
                color: color_arr,
            },
            Vertex {
                position: ndc(x1, y1),
                uv: [1.0, 1.0],
                color: color_arr,
            },
            Vertex {
                position: ndc(x0, y1),
                uv: [0.0, 1.0],
                color: color_arr,
            },
        ];

        queue.write_buffer(
            &self.vertex_buffer,
            0,
            vertices_to_bytes(&vertices),
        );

        Ok(())
    }

    /// Render one frame: submit all accumulated sprite draws and present.
    ///
    /// This is the rendering close-loop entry. Call `begin_frame()` then one or
    /// more `draw_sprite()` calls, then `render()`.
    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("deusex-renderer frame encoder"),
            });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("deusex-renderer sprite pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            rpass.set_pipeline(&self.pipeline);
            rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rpass.set_index_buffer(
                self.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );

            if let Some(bg) = &self.current_bind_group {
                rpass.set_bind_group(0, bg, &[]);
            }

            rpass.draw_indexed(0..self.index_count, 0, 0..1);
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        Ok(())
    }

    /// Return the number of textures currently uploaded.
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gpu_init() {
        // Gpu::new() should initialize a wgpu device and queue.
        // In CI environments this may fall back to llvmpipe/software rendering.
        let gpu = Gpu::new().await.expect("failed to create wgpu GPU");
        // If we got here, device and queue were created successfully.
        let _ = gpu;
    }

    #[test]
    fn test_texture_id_null() {
        let id = TextureId::null();
        assert!(id.is_null());

        let id = TextureId(1);
        assert!(!id.is_null());
    }
}
