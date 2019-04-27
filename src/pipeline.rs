mod cache;

pub use cache::Cache;

use std::mem;
use std::rc::Rc;

use glyph_brush::rusttype::{point, Rect};

pub struct Pipeline {
    transform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    cache: Rc<Cache>,
    uniform_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::BindGroup,
    instances: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    current_instances: u32,
}

impl Pipeline {
    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub const IDENTITY_MATRIX: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    pub fn new(
        device: &wgpu::Device,
        filter_mode: wgpu::FilterMode,
        cache_width: u32,
        cache_height: u32,
    ) -> Pipeline {
        let transform = device
            .create_buffer_mapped(
                16,
                wgpu::BufferUsageFlags::UNIFORM
                    | wgpu::BufferUsageFlags::TRANSFER_DST,
            )
            .fill_from_slice(&Self::IDENTITY_MATRIX);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            r_address_mode: wgpu::AddressMode::ClampToEdge,
            s_address_mode: wgpu::AddressMode::ClampToEdge,
            t_address_mode: wgpu::AddressMode::ClampToEdge,
            mag_filter: filter_mode,
            min_filter: filter_mode,
            mipmap_filter: filter_mode,
            lod_min_clamp: -100.0,
            lod_max_clamp: 100.0,
            max_anisotropy: 0,
            compare_function: wgpu::CompareFunction::Always,
            border_color: wgpu::BorderColor::TransparentBlack,
        });

        let cache = Cache::new(device, cache_width, cache_height);

        let uniform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[
                    wgpu::BindGroupLayoutBinding {
                        binding: 0,
                        visibility: wgpu::ShaderStageFlags::VERTEX,
                        ty: wgpu::BindingType::UniformBuffer,
                    },
                    wgpu::BindGroupLayoutBinding {
                        binding: 1,
                        visibility: wgpu::ShaderStageFlags::FRAGMENT,
                        ty: wgpu::BindingType::Sampler,
                    },
                    wgpu::BindGroupLayoutBinding {
                        binding: 2,
                        visibility: wgpu::ShaderStageFlags::FRAGMENT,
                        ty: wgpu::BindingType::SampledTexture,
                    },
                ],
            });

        let uniforms = Self::create_uniforms(
            device,
            &uniform_layout,
            &transform,
            &sampler,
            &cache.view,
        );

        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            size: mem::size_of::<Instance>() as u32 * Instance::MAX as u32,
            usage: wgpu::BufferUsageFlags::VERTEX
                | wgpu::BufferUsageFlags::TRANSFER_DST,
        });

        let layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                bind_group_layouts: &[&uniform_layout],
            });

        let vs_module =
            device.create_shader_module(include_bytes!("shader/vertex.spv"));
        let fs_module =
            device.create_shader_module(include_bytes!("shader/fragment.spv"));

        let pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                layout: &layout,
                vertex_stage: wgpu::PipelineStageDescriptor {
                    module: &vs_module,
                    entry_point: "main",
                },
                fragment_stage: wgpu::PipelineStageDescriptor {
                    module: &fs_module,
                    entry_point: "main",
                },
                rasterization_state: wgpu::RasterizationStateDescriptor {
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: wgpu::CullMode::None,
                    depth_bias: 0,
                    depth_bias_slope_scale: 0.0,
                    depth_bias_clamp: 0.0,
                },
                primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
                color_states: &[wgpu::ColorStateDescriptor {
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    color: wgpu::BlendDescriptor {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendDescriptor {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    write_mask: wgpu::ColorWriteFlags::ALL,
                }],
                depth_stencil_state: None,
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[wgpu::VertexBufferDescriptor {
                    stride: mem::size_of::<Instance>() as u32,
                    step_mode: wgpu::InputStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttributeDescriptor {
                            attribute_index: 0,
                            format: wgpu::VertexFormat::Float3,
                            offset: 0,
                        },
                        wgpu::VertexAttributeDescriptor {
                            attribute_index: 1,
                            format: wgpu::VertexFormat::Float2,
                            offset: 4 * 3,
                        },
                        wgpu::VertexAttributeDescriptor {
                            attribute_index: 2,
                            format: wgpu::VertexFormat::Float2,
                            offset: 4 * (3 + 2),
                        },
                        wgpu::VertexAttributeDescriptor {
                            attribute_index: 3,
                            format: wgpu::VertexFormat::Float2,
                            offset: 4 * (3 + 2 + 2),
                        },
                        wgpu::VertexAttributeDescriptor {
                            attribute_index: 4,
                            format: wgpu::VertexFormat::Float4,
                            offset: 4 * (3 + 2 + 2 + 2),
                        },
                    ],
                }],
                sample_count: 1,
            });

        Pipeline {
            transform,
            sampler,
            cache: Rc::new(cache),
            uniform_layout,
            uniforms,
            instances,
            pipeline,
            current_instances: 0,
        }
    }

    pub fn cache(&self) -> Rc<Cache> {
        self.cache.clone()
    }

    pub fn increase_cache_size(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) {
        self.cache = Rc::new(Cache::new(device, width, height));

        self.uniforms = Self::create_uniforms(
            device,
            &self.uniform_layout,
            &self.transform,
            &self.sampler,
            &self.cache.view,
        );
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        transform: [f32; 16],
        instances: &[Instance],
        target: &wgpu::TextureView,
    ) {
        let transform_buffer = device
            .create_buffer_mapped(16, wgpu::BufferUsageFlags::TRANSFER_SRC)
            .fill_from_slice(&transform[..]);

        encoder.copy_buffer_to_buffer(
            &transform_buffer,
            0,
            &self.transform,
            0,
            16 * 4,
        );

        let instance_buffer = device
            .create_buffer_mapped(
                instances.len(),
                wgpu::BufferUsageFlags::TRANSFER_SRC,
            )
            .fill_from_slice(instances);

        encoder.copy_buffer_to_buffer(
            &instance_buffer,
            0,
            &self.instances,
            0,
            (mem::size_of::<Instance>() * instances.len()) as u32,
        );

        self.current_instances = instances.len() as u32;

        self.redraw(encoder, target);
    }

    pub fn redraw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let mut render_pass =
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[
                    wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: target,
                        load_op: wgpu::LoadOp::Load,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        },
                    },
                ],
                depth_stencil_attachment: None,
            });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniforms);
        render_pass.set_vertex_buffers(&[(&self.instances, 0)]);

        render_pass.draw(0..4, 0..self.current_instances as u32);
    }

    // Helpers
    fn create_uniforms(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        transform: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        cache: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: transform,
                        range: 0..64,
                    },
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(cache),
                },
            ],
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Instance {
    left_top: [f32; 3],
    right_bottom: [f32; 2],
    tex_left_top: [f32; 2],
    tex_right_bottom: [f32; 2],
    color: [f32; 4],
}

impl Instance {
    const MAX: usize = 50_000;
}

impl From<glyph_brush::GlyphVertex> for Instance {
    #[inline]
    fn from(vertex: glyph_brush::GlyphVertex) -> Instance {
        let glyph_brush::GlyphVertex {
            mut tex_coords,
            pixel_coords,
            bounds,
            screen_dimensions: (screen_w, screen_h),
            color,
            z,
        } = vertex;

        let gl_bounds = Rect {
            min: point(
                2.0 * (bounds.min.x / screen_w - 0.5),
                2.0 * (bounds.min.y / screen_h - 0.5),
            ),
            max: point(
                2.0 * (bounds.max.x / screen_w - 0.5),
                2.0 * (bounds.max.y / screen_h - 0.5),
            ),
        };

        let mut gl_rect = Rect {
            min: point(
                2.0 * (pixel_coords.min.x as f32 / screen_w - 0.5),
                2.0 * (pixel_coords.min.y as f32 / screen_h - 0.5),
            ),
            max: point(
                2.0 * (pixel_coords.max.x as f32 / screen_w - 0.5),
                2.0 * (pixel_coords.max.y as f32 / screen_h - 0.5),
            ),
        };

        // handle overlapping bounds, modify uv_rect to preserve texture aspect
        if gl_rect.max.x > gl_bounds.max.x {
            let old_width = gl_rect.width();
            gl_rect.max.x = gl_bounds.max.x;
            tex_coords.max.x = tex_coords.min.x
                + tex_coords.width() * gl_rect.width() / old_width;
        }

        if gl_rect.min.x < gl_bounds.min.x {
            let old_width = gl_rect.width();
            gl_rect.min.x = gl_bounds.min.x;
            tex_coords.min.x = tex_coords.max.x
                - tex_coords.width() * gl_rect.width() / old_width;
        }

        if gl_rect.max.y > gl_bounds.max.y {
            let old_height = gl_rect.height();
            gl_rect.max.y = gl_bounds.max.y;
            tex_coords.max.y = tex_coords.min.y
                + tex_coords.height() * gl_rect.height() / old_height;
        }

        if gl_rect.min.y < gl_bounds.min.y {
            let old_height = gl_rect.height();
            gl_rect.min.y = gl_bounds.min.y;
            tex_coords.min.y = tex_coords.max.y
                - tex_coords.height() * gl_rect.height() / old_height;
        }

        Instance {
            left_top: [gl_rect.min.x, gl_rect.max.y, z],
            right_bottom: [gl_rect.max.x, gl_rect.min.y],
            tex_left_top: [tex_coords.min.x, tex_coords.max.y],
            tex_right_bottom: [tex_coords.max.x, tex_coords.min.y],
            color,
        }
    }
}