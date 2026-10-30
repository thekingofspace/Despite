use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use crate::prelude::{
    FirstExtraInputBinding, FragmentEntryPoint, GlobalsBinding, GlobalsData, InputBinding,
    LinearSamplerBinding, MaxExtraInputs, NearestSamplerBinding, ParamsBinding,
    RepeatSamplerBinding, SamplerBinding, VertexEntryPoint, pack_params,
};
use crate::types::{ShaderError, ShaderImage, ShaderJob, ShaderOutput};
use crate::validate::{assemble_source, check_wgsl, validate_settings};

const OutputFormat: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const FrameTimeout: Duration = Duration::from_secs(30);

struct SamplerSet {
    LinearClamp: wgpu::Sampler,
    NearestClamp: wgpu::Sampler,
    LinearRepeat: wgpu::Sampler,
    NearestRepeat: wgpu::Sampler,
}

pub struct ShaderRunner {
    Adapter: wgpu::Adapter,
    Device: wgpu::Device,
    Queue: wgpu::Queue,
    Samplers: SamplerSet,
}

struct FrameTarget {
    Texture: wgpu::Texture,
    View: wgpu::TextureView,
    Readback: wgpu::Buffer,
    Width: u32,
    Height: u32,
    PaddedRowBytes: u32,
}

fn wgpu_error(Context: &str, Error: wgpu::Error) -> ShaderError {
    ShaderError::new(format!(
        "{Context}: {}",
        crate::validate::error_chain(&Error)
    ))
}

fn make_sampler(
    Device: &wgpu::Device,
    Label: &str,
    Filter: wgpu::FilterMode,
    Address: wgpu::AddressMode,
) -> wgpu::Sampler {
    Device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(Label),
        address_mode_u: Address,
        address_mode_v: Address,
        address_mode_w: Address,
        mag_filter: Filter,
        min_filter: Filter,
        ..Default::default()
    })
}

fn pick_adapter(Instance: &wgpu::Instance) -> Result<wgpu::Adapter, ShaderError> {
    let Preferred = pollster::block_on(Instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    }));
    match Preferred {
        Ok(Adapter) => return Ok(Adapter),
        Err(Error) => log::warn!("despite-shader: no high-performance adapter: {Error}"),
    }
    let Fallback = pollster::block_on(Instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::None,
        force_fallback_adapter: true,
        compatible_surface: None,
        apply_limit_buckets: false,
    }));
    match Fallback {
        Ok(Adapter) => return Ok(Adapter),
        Err(Error) => log::warn!("despite-shader: no fallback adapter: {Error}"),
    }
    let mut Adapters = pollster::block_on(Instance.enumerate_adapters(wgpu::Backends::all()));
    Adapters.sort_by_key(|Adapter| match Adapter.get_info().device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Cpu => 3,
        wgpu::DeviceType::Other => 4,
    });
    Adapters
        .into_iter()
        .next()
        .ok_or_else(|| ShaderError::new("No GPU adapter is available for shader rendering"))
}

fn request_device(Adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), ShaderError> {
    let Full = pollster::block_on(Adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("despite-shader"),
        required_limits: Adapter.limits(),
        ..Default::default()
    }));
    match Full {
        Ok(Pair) => return Ok(Pair),
        Err(Error) => log::warn!("despite-shader: device with full adapter limits failed: {Error}"),
    }
    let Modest = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(Adapter.limits());
    pollster::block_on(Adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("despite-shader"),
        required_limits: Modest,
        ..Default::default()
    }))
    .map_err(|Error| ShaderError::new(format!("Could not open the GPU device: {Error}")))
}

impl ShaderRunner {
    pub fn new_headless() -> Result<ShaderRunner, ShaderError> {
        let Instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Adapter = pick_adapter(&Instance)?;
        let (Device, Queue) = request_device(&Adapter)?;
        Device.on_uncaptured_error(Arc::new(|Error: wgpu::Error| {
            log::error!("despite-shader: uncaptured GPU error: {Error}");
        }));
        Device.set_device_lost_callback(|Reason, Message| {
            log::error!("despite-shader: GPU device lost ({Reason:?}): {Message}");
        });
        let Samplers = SamplerSet {
            LinearClamp: make_sampler(
                &Device,
                "LinearClamp",
                wgpu::FilterMode::Linear,
                wgpu::AddressMode::ClampToEdge,
            ),
            NearestClamp: make_sampler(
                &Device,
                "NearestClamp",
                wgpu::FilterMode::Nearest,
                wgpu::AddressMode::ClampToEdge,
            ),
            LinearRepeat: make_sampler(
                &Device,
                "LinearRepeat",
                wgpu::FilterMode::Linear,
                wgpu::AddressMode::Repeat,
            ),
            NearestRepeat: make_sampler(
                &Device,
                "NearestRepeat",
                wgpu::FilterMode::Nearest,
                wgpu::AddressMode::Repeat,
            ),
        };
        let Info = Adapter.get_info();
        log::info!(
            "despite-shader: using {} ({:?}, {:?})",
            Info.name,
            Info.backend,
            Info.device_type
        );
        Ok(ShaderRunner {
            Adapter,
            Device,
            Queue,
            Samplers,
        })
    }

    pub fn adapter_name(&self) -> String {
        let Info = self.Adapter.get_info();
        format!("{} ({:?})", Info.name, Info.backend)
    }

    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.Adapter.get_info()
    }

    pub fn max_extra_inputs(&self) -> usize {
        let Limits = self.Device.limits();
        let ByTextures = Limits
            .max_sampled_textures_per_shader_stage
            .saturating_sub(1) as usize;
        let ByBindings = Limits
            .max_bindings_per_bind_group
            .saturating_sub(FirstExtraInputBinding) as usize;
        ByTextures.min(ByBindings).min(MaxExtraInputs)
    }

    pub fn max_texture_size(&self) -> u32 {
        self.Device.limits().max_texture_dimension_2d
    }

    fn check_limits(&self, Job: &ShaderJob) -> Result<(), ShaderError> {
        let Limits = self.Device.limits();
        let MaxSide = Limits.max_texture_dimension_2d;
        let Allowed = self.max_extra_inputs();
        if Job.Inputs.len() > Allowed {
            return Err(ShaderError::new(format!(
                "Too many extra input textures: {} (this GPU allows at most {Allowed})",
                Job.Inputs.len()
            )));
        }
        let (OutputWidth, OutputHeight) = Job.output_size();
        if OutputWidth > MaxSide || OutputHeight > MaxSide {
            return Err(ShaderError::new(format!(
                "Output size {OutputWidth}x{OutputHeight} exceeds this GPU's maximum texture size of {MaxSide}x{MaxSide}"
            )));
        }
        let mut Images: Vec<(String, &ShaderImage)> =
            vec![("The main image".to_string(), &Job.Main)];
        for Input in &Job.Inputs {
            Images.push((format!("Input texture `{}`", Input.Name), &Input.Image));
        }
        for (Label, Image) in Images {
            if Image.Width > MaxSide || Image.Height > MaxSide {
                return Err(ShaderError::new(format!(
                    "{Label} is {}x{}, which exceeds this GPU's maximum texture size of {MaxSide}x{MaxSide}",
                    Image.Width, Image.Height
                )));
            }
        }
        let PaddedRowBytes = padded_row_bytes(OutputWidth);
        let ReadbackSize = PaddedRowBytes as u64 * OutputHeight as u64;
        if ReadbackSize > Limits.max_buffer_size {
            return Err(ShaderError::new(format!(
                "Output size {OutputWidth}x{OutputHeight} needs a {ReadbackSize}-byte readback buffer, more than this GPU allows ({})",
                Limits.max_buffer_size
            )));
        }
        Ok(())
    }

    fn upload_image(&self, Label: &str, Image: &ShaderImage) -> wgpu::TextureView {
        let Size = wgpu::Extent3d {
            width: Image.Width,
            height: Image.Height,
            depth_or_array_layers: 1,
        };
        let Texture = self.Device.create_texture(&wgpu::TextureDescriptor {
            label: Some(Label),
            size: Size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OutputFormat,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.Queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &Texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &Image.Rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(Image.Width * 4),
                rows_per_image: Some(Image.Height),
            },
            Size,
        );
        Texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn create_target(&self, Width: u32, Height: u32) -> FrameTarget {
        let Texture = self.Device.create_texture(&wgpu::TextureDescriptor {
            label: Some("despite-shader output"),
            size: wgpu::Extent3d {
                width: Width,
                height: Height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OutputFormat,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let View = Texture.create_view(&wgpu::TextureViewDescriptor::default());
        let PaddedRowBytes = padded_row_bytes(Width);
        let Readback = self.Device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("despite-shader readback"),
            size: PaddedRowBytes as u64 * Height as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        FrameTarget {
            Texture,
            View,
            Readback,
            Width,
            Height,
            PaddedRowBytes,
        }
    }

    fn create_layout(&self, ExtraInputs: usize) -> wgpu::BindGroupLayout {
        let Uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let SamplerType = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        let TextureType = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let mut Entries = vec![
            (GlobalsBinding, Uniform),
            (ParamsBinding, Uniform),
            (SamplerBinding, SamplerType),
            (LinearSamplerBinding, SamplerType),
            (NearestSamplerBinding, SamplerType),
            (RepeatSamplerBinding, SamplerType),
            (InputBinding, TextureType),
        ];
        for Index in 0..ExtraInputs {
            Entries.push((FirstExtraInputBinding + Index as u32, TextureType));
        }
        let LayoutEntries: Vec<wgpu::BindGroupLayoutEntry> = Entries
            .into_iter()
            .map(|(Binding, Kind)| wgpu::BindGroupLayoutEntry {
                binding: Binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: Kind,
                count: None,
            })
            .collect();
        self.Device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("despite-shader bindings"),
                entries: &LayoutEntries,
            })
    }

    fn create_pipeline(
        &self,
        Source: &str,
        Layout: &wgpu::BindGroupLayout,
    ) -> Result<wgpu::RenderPipeline, ShaderError> {
        let Scope = self.Device.push_error_scope(wgpu::ErrorFilter::Validation);
        let Module = self
            .Device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("despite-shader effect"),
                source: wgpu::ShaderSource::Wgsl(Source.into()),
            });
        let PipelineLayout = self
            .Device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("despite-shader pipeline layout"),
                bind_group_layouts: &[Some(Layout)],
                immediate_size: 0,
            });
        let Pipeline = self
            .Device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("despite-shader pipeline"),
                layout: Some(&PipelineLayout),
                vertex: wgpu::VertexState {
                    module: &Module,
                    entry_point: Some(VertexEntryPoint),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &Module,
                    entry_point: Some(FragmentEntryPoint),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: OutputFormat,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        if let Some(Error) = pollster::block_on(Scope.pop()) {
            return Err(wgpu_error("The GPU rejected the shader", Error));
        }
        Ok(Pipeline)
    }

    pub fn run(&mut self, Job: &ShaderJob) -> Result<ShaderOutput, ShaderError> {
        validate_settings(Job)?;
        self.check_limits(Job)?;
        let Assembled = assemble_source(Job);
        check_wgsl(&Assembled)?;

        let MemoryScope = self.Device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let Outcome = self.render_frames(Job, &Assembled.Text);
        let MemoryError = pollster::block_on(MemoryScope.pop());
        if let Some(Error) = MemoryError {
            return Err(wgpu_error("The GPU ran out of memory", Error));
        }
        Outcome
    }

    fn render_frames(&self, Job: &ShaderJob, Source: &str) -> Result<ShaderOutput, ShaderError> {
        let Layout = self.create_layout(Job.Inputs.len());
        let Pipeline = self.create_pipeline(Source, &Layout)?;

        let Scope = self.Device.push_error_scope(wgpu::ErrorFilter::Validation);
        let (OutputWidth, OutputHeight) = Job.output_size();
        let MainView = self.upload_image("despite-shader Input", &Job.Main);
        let ExtraViews: Vec<wgpu::TextureView> = Job
            .Inputs
            .iter()
            .map(|Input| self.upload_image(&Input.Name, &Input.Image))
            .collect();
        let GlobalsBuffer = self.Device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("despite-shader Globals"),
            size: std::mem::size_of::<GlobalsData>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ParamBytes = pack_params(&Job.Params);
        let ParamsBuffer = self.Device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("despite-shader Params"),
            size: ParamBytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.Queue.write_buffer(&ParamsBuffer, 0, &ParamBytes);
        let (DefaultSampler, DefaultRepeat) = if Job.SmoothSampling {
            (&self.Samplers.LinearClamp, &self.Samplers.LinearRepeat)
        } else {
            (&self.Samplers.NearestClamp, &self.Samplers.NearestRepeat)
        };
        let mut Entries = vec![
            wgpu::BindGroupEntry {
                binding: GlobalsBinding,
                resource: GlobalsBuffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: ParamsBinding,
                resource: ParamsBuffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: SamplerBinding,
                resource: wgpu::BindingResource::Sampler(DefaultSampler),
            },
            wgpu::BindGroupEntry {
                binding: LinearSamplerBinding,
                resource: wgpu::BindingResource::Sampler(&self.Samplers.LinearClamp),
            },
            wgpu::BindGroupEntry {
                binding: NearestSamplerBinding,
                resource: wgpu::BindingResource::Sampler(&self.Samplers.NearestClamp),
            },
            wgpu::BindGroupEntry {
                binding: RepeatSamplerBinding,
                resource: wgpu::BindingResource::Sampler(DefaultRepeat),
            },
            wgpu::BindGroupEntry {
                binding: InputBinding,
                resource: wgpu::BindingResource::TextureView(&MainView),
            },
        ];
        for (Index, View) in ExtraViews.iter().enumerate() {
            Entries.push(wgpu::BindGroupEntry {
                binding: FirstExtraInputBinding + Index as u32,
                resource: wgpu::BindingResource::TextureView(View),
            });
        }
        let BindGroup = self.Device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("despite-shader bind group"),
            layout: &Layout,
            entries: &Entries,
        });
        let Target = self.create_target(OutputWidth, OutputHeight);
        if let Some(Error) = pollster::block_on(Scope.pop()) {
            return Err(wgpu_error("Could not prepare GPU resources", Error));
        }

        let Range = match Job.OnlyFrame {
            Some(Only) => Only..Only + 1,
            None => 0..Job.FrameCount,
        };
        let mut Frames = Vec::with_capacity(Range.len());
        for Frame in Range {
            let Globals = GlobalsData::for_frame(
                (OutputWidth, OutputHeight),
                (Job.Main.Width, Job.Main.Height),
                Frame,
                Job.FrameCount,
                Job.Fps,
            );
            self.Queue
                .write_buffer(&GlobalsBuffer, 0, bytemuck::bytes_of(&Globals));
            Frames.push(self.render_frame(&Pipeline, &BindGroup, &Target)?);
        }
        Ok(ShaderOutput { Frames })
    }

    fn render_frame(
        &self,
        Pipeline: &wgpu::RenderPipeline,
        BindGroup: &wgpu::BindGroup,
        Target: &FrameTarget,
    ) -> Result<ShaderImage, ShaderError> {
        let Scope = self.Device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut Encoder = self
            .Device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("despite-shader frame"),
            });
        {
            let mut Pass = Encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("despite-shader effect pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &Target.View,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            Pass.set_pipeline(Pipeline);
            Pass.set_bind_group(0, BindGroup, &[]);
            Pass.draw(0..3, 0..1);
        }
        Encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &Target.Texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &Target.Readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(Target.PaddedRowBytes),
                    rows_per_image: Some(Target.Height),
                },
            },
            wgpu::Extent3d {
                width: Target.Width,
                height: Target.Height,
                depth_or_array_layers: 1,
            },
        );
        let Submission = self.Queue.submit([Encoder.finish()]);
        if let Some(Error) = pollster::block_on(Scope.pop()) {
            return Err(wgpu_error("The GPU rejected the render commands", Error));
        }

        let (Sender, Receiver) = mpsc::channel();
        Target
            .Readback
            .map_async(wgpu::MapMode::Read, .., move |Outcome| {
                let _ = Sender.send(Outcome);
            });
        self.Device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(Submission),
                timeout: Some(FrameTimeout),
            })
            .map_err(|Error| ShaderError::new(format!("Waiting for the GPU failed: {Error}")))?;
        let Mapped = Receiver
            .recv_timeout(FrameTimeout)
            .map_err(|_| ShaderError::new("The GPU did not finish rendering the frame in time"))?;
        Mapped.map_err(|Error| {
            ShaderError::new(format!(
                "Could not read the frame back from the GPU: {Error}"
            ))
        })?;

        let RowBytes = Target.Width as usize * 4;
        let mut Rgba = Vec::with_capacity(RowBytes * Target.Height as usize);
        {
            let View = Target.Readback.get_mapped_range(..).map_err(|Error| {
                ShaderError::new(format!("Could not access the frame readback: {Error}"))
            })?;
            for Row in View.chunks_exact(Target.PaddedRowBytes as usize) {
                Rgba.extend_from_slice(&Row[..RowBytes]);
            }
        }
        Target.Readback.unmap();
        Ok(ShaderImage {
            Width: Target.Width,
            Height: Target.Height,
            Rgba,
        })
    }
}

pub(crate) fn padded_row_bytes(Width: u32) -> u32 {
    let Unpadded = Width * 4;
    let Alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    Unpadded.div_ceil(Alignment) * Alignment
}
