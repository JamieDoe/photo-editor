//! Phase 0 GPU feasibility spike (see docs/ADR/0004-gpu-evaluation.md).
//!
//! Implements [`renderer::RenderBackend`] with a wgpu compute shader so the same
//! [`RenderPlan`] runs on CPU or GPU. Not production code: no pipeline caching across
//! sizes, no tiling for images beyond the device's buffer limits, RGBA output only.

use bytemuck::{Pod, Zeroable};
use image_core::{Cancellation, LinearImage, OutputImage, PixelFormat};
use renderer::{RenderBackend, RenderError, RenderPlan, Stage};
use wgpu::util::DeviceExt;

const WORKGROUP: u32 = 256;
const MAX_GROUPS_X: u32 = 65_535;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    gains: [f32; 4],
    contrast_gamma: f32,
    saturation: f32,
    flags: u32,
    pixel_count: u32,
}

/// A source image uploaded to GPU memory once and re-rendered many times, which is
/// how a GPU preview path would hold a pyramid level while a slider moves.
pub struct ResidentSource {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
}

pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    adapter_name: String,
}

impl GpuRenderer {
    /// Returns `None` when no usable adapter exists (the CPU path must then be used).
    pub fn new() -> Option<Self> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .ok()?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gpu-spike"),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .ok()?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render-plan"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("render-plan"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        Some(Self {
            device,
            queue,
            pipeline,
            layout,
            adapter_name: format!("{} ({:?})", info.name, info.backend),
        })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    pub fn upload(&self, source: &LinearImage) -> ResidentSource {
        let mut bytes: Vec<u8> = bytemuck::cast_slice(source.data()).to_vec();
        bytes.resize(bytes.len().next_multiple_of(4), 0);
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("source"),
                contents: &bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        ResidentSource {
            buffer,
            width: source.width(),
            height: source.height(),
        }
    }

    /// Renders an already-uploaded source and reads the result back to CPU memory.
    pub fn render_resident(
        &self,
        plan: &RenderPlan,
        source: &ResidentSource,
    ) -> Result<OutputImage, RenderError> {
        let params = params_for(plan, source.width * source.height)?;
        let pixel_count = u64::from(source.width) * u64::from(source.height);
        let out_size = pixel_count * 4;
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output"),
            size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: out_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render-plan"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: source.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });

        let groups = (pixel_count as u32).div_ceil(WORKGROUP);
        let (gx, gy) = (groups.min(MAX_GROUPS_X), groups.div_ceil(MAX_GROUPS_X));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, out_size);
        self.queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Backend(format!("poll: {e}")))?;
        rx.recv()
            .map_err(|e| RenderError::Backend(e.to_string()))?
            .map_err(|e| RenderError::Backend(format!("map: {e}")))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Backend(format!("map range: {e}")))?
            .to_vec();
        readback.unmap();
        OutputImage::from_raw(source.width, source.height, PixelFormat::Rgba8, data)
            .map_err(|e| RenderError::Backend(e.to_string()))
    }
}

impl RenderBackend for GpuRenderer {
    fn name(&self) -> &'static str {
        "wgpu-spike"
    }

    fn render(
        &self,
        plan: &RenderPlan,
        source: &LinearImage,
        format: PixelFormat,
        cancel: &dyn Cancellation,
    ) -> Result<OutputImage, RenderError> {
        if format != PixelFormat::Rgba8 {
            return Err(RenderError::Backend(
                "spike supports RGBA8 output only".into(),
            ));
        }
        if cancel.is_cancelled() {
            return Err(RenderError::Cancelled);
        }
        // A submitted GPU pass cannot be interrupted; cancellation is per dispatch.
        self.render_resident(plan, &self.upload(source))
    }
}

/// Maps the plan onto the shader's fixed stage order. Returns an error for plans the
/// spike's single fused kernel cannot express.
fn params_for(plan: &RenderPlan, pixel_count: u32) -> Result<Params, RenderError> {
    let mut p = Params {
        gains: [1.0, 1.0, 1.0, 1.0],
        contrast_gamma: 1.0,
        saturation: 1.0,
        flags: 0,
        pixel_count,
    };
    let mut phase = 0; // 0: gains, 1: contrast, 2: saturation
    for stage in &plan.stages {
        match *stage {
            Stage::WhiteBalance { gains } if phase == 0 => {
                for (g, wb) in p.gains.iter_mut().zip(gains) {
                    *g *= wb;
                }
            }
            Stage::Exposure { multiplier } if phase == 0 => {
                for g in &mut p.gains[..3] {
                    *g *= multiplier;
                }
            }
            Stage::Contrast { gamma } if phase <= 1 => {
                p.contrast_gamma = gamma;
                p.flags |= 1;
                phase = 1;
            }
            Stage::Saturation { factor } if phase <= 2 => {
                p.saturation = factor;
                p.flags |= 2;
                phase = 2;
            }
            ref s => {
                return Err(RenderError::Backend(format!(
                    "stage {} out of supported order",
                    s.name()
                )));
            }
        }
    }
    Ok(p)
}
