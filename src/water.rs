//! Uses Rune's GPU compositor/backdrop facility, as ui-lab's water study does.
//! Preserve the original alpha: rune-fx Shockwave currently replaces it with 1.
use crate::State;
use rune::compose::{Compositor, DeviceKey, Frame};
use rune::render::{EffectPhase, EffectSite};
struct Gpu {
    key: DeviceKey,
    shader: wgpu::ShaderModule,
    uniform: wgpu::Buffer,
    bindings: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    pipelines: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
}
impl Gpu {
    fn new(device: &wgpu::Device, key: DeviceKey) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water refraction"),
            source: wgpu::ShaderSource::Wgsl(include_str!("water.wgsl").into()),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water taps"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bindings = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water"),
            bind_group_layouts: &[Some(&bindings)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            key,
            shader,
            uniform,
            bindings,
            layout,
            sampler,
            pipelines: vec![],
        }
    }
    fn pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) -> usize {
        if let Some(i) = self.pipelines.iter().position(|(f, _)| *f == format) {
            return i;
        }
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water refraction"),
            layout: Some(&self.layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.push((format, pipeline));
        self.pipelines.len() - 1
    }
}
#[derive(Default)]
pub struct Water {
    gpu: Option<Gpu>,
    moving: bool,
}
impl Compositor<State> for Water {
    fn animating(&self) -> bool {
        self.moving
    }
    fn render(&mut self, f: &mut Frame<'_, State>) {
        self.moving = !f.rt.state.ripples.is_empty();
        if !self.moving {
            f.draw_ui();
            return;
        }
        let key = f.device_key();
        if self.gpu.as_ref().is_none_or(|g| g.key != key) {
            self.gpu = Some(Gpu::new(f.device, key));
        }
        let gpu = self.gpu.as_mut().unwrap();
        let mut values = vec![f.size.0 as f32, f.size.1 as f32, f.rt.env.scale, 0.];
        for i in 0..5 {
            if let Some((p, t)) = f.rt.state.ripples.get(i) {
                values.extend([
                    p[0] + if matches!(
                        f.rt.state.dock.edge,
                        crate::docking::Edge::Left | crate::docking::Edge::Right
                    ) {
                        0.
                    } else {
                        crate::SHOULDER
                    },
                    p[1],
                    (f.rt.state.clock - t) as f32,
                    1.,
                ]);
            } else {
                values.extend([0.; 4]);
            }
        }
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
        f.write_buffer(&gpu.uniform, 0, &bytes);
        let mut group = None;
        f.ui.prepare(
            f.device,
            f.queue,
            &mut f.rt.text.atlas,
            &f.rt.instances,
            f.size,
        );
        f.ui.draw_with_backdrop(
            f.encoder,
            f.target,
            f.rt.clear_color(),
            &mut |phase, site: &EffectSite<'_>| {
                if site.mark != 77 {
                    return;
                }
                match phase {
                    EffectPhase::Backdrop(_) => {
                        if let Some(view) = site.backdrop {
                            group = Some(f.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("water backdrop"),
                                layout: &gpu.bindings,
                                entries: &[
                                    wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: gpu.uniform.as_entire_binding(),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 1,
                                        resource: wgpu::BindingResource::TextureView(view),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 2,
                                        resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                                    },
                                ],
                            }));
                        }
                    }
                    EffectPhase::Draw(pass) => {
                        if let Some(group) = &group {
                            let i = gpu.pipeline(f.device, site.format);
                            pass.set_viewport(
                                0.,
                                0.,
                                site.size[0] as f32,
                                site.size[1] as f32,
                                0.,
                                1.,
                            );
                            pass.set_pipeline(&gpu.pipelines[i].1);
                            pass.set_bind_group(0, group, &[]);
                            pass.draw(0..3, 0..1);
                        }
                    }
                }
            },
        );
    }
}
