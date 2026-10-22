//! ClearWater6.1 water study. GPU FFT and photon transport, sampled by a projected grid.
//! Call prepare outside the render pass, draw inside it. Reuse a slot only after its fence.
use crate::vk_renderer::Buffer;
use ash::vk;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WaterCamera {
    pub eye_time: [f32; 4],
    pub forward_aspect: [f32; 4],
    pub right_depth: [f32; 4],
    pub up_exposure: [f32; 4],
}
struct Texture {
    device: ash::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    size: u32,
    layers: u32,
    levels: u32,
}
impl Texture {
    unsafe fn new(
        d: &ash::Device,
        props: &vk::PhysicalDeviceMemoryProperties,
        size: u32,
        layers: u32,
        levels: u32,
    ) -> Result<Self> {
        Self::new_format(
            d,
            props,
            size,
            layers,
            levels,
            vk::Format::R16G16B16A16_SFLOAT,
            false,
        )
    }
    unsafe fn new_format(
        d: &ash::Device,
        props: &vk::PhysicalDeviceMemoryProperties,
        size: u32,
        layers: u32,
        levels: u32,
        format: vk::Format,
        render: bool,
    ) -> Result<Self> {
        let mut t = Self {
            device: d.clone(),
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            view: vk::ImageView::null(),
            size,
            layers,
            levels,
        };
        t.image = d.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D {
                    width: size,
                    height: size,
                    depth: 1,
                })
                .mip_levels(levels)
                .array_layers(layers)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | if render {
                            vk::ImageUsageFlags::COLOR_ATTACHMENT
                        } else {
                            vk::ImageUsageFlags::empty()
                        },
                ),
            None,
        )?;
        let req = d.get_image_memory_requirements(t.image);
        let ty = (0..props.memory_type_count)
            .find(|i| {
                req.memory_type_bits & (1 << i) != 0
                    && props.memory_types[*i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            })
            .ok_or("No water texture memory")?;
        t.memory = d.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(ty),
            None,
        )?;
        d.bind_image_memory(t.image, t.memory, 0)?;
        t.view = d.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(t.image)
                .view_type(if layers > 1 {
                    vk::ImageViewType::TYPE_2D_ARRAY
                } else {
                    vk::ImageViewType::TYPE_2D
                })
                .format(format)
                .subresource_range(t.range(0, levels)),
            None,
        )?;
        Ok(t)
    }
    fn range(&self, base: u32, count: u32) -> vk::ImageSubresourceRange {
        vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(base)
            .level_count(count)
            .layer_count(self.layers)
    }
    unsafe fn barrier(
        &self,
        cmd: vk::CommandBuffer,
        base: u32,
        count: u32,
        old: vk::ImageLayout,
        new: vk::ImageLayout,
        src: vk::PipelineStageFlags,
        dst: vk::PipelineStageFlags,
        read: vk::AccessFlags,
        write: vk::AccessFlags,
    ) {
        self.device.cmd_pipeline_barrier(
            cmd,
            src,
            dst,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .image(self.image)
                .subresource_range(self.range(base, count))
                .old_layout(old)
                .new_layout(new)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .src_access_mask(read)
                .dst_access_mask(write)],
        );
    }
    unsafe fn upload(&self, cmd: vk::CommandBuffer, source: vk::Buffer, initialized: bool) {
        let d = &self.device;
        self.barrier(
            cmd,
            0,
            self.levels,
            if initialized {
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            } else {
                vk::ImageLayout::UNDEFINED
            },
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            if initialized {
                vk::PipelineStageFlags::ALL_COMMANDS
            } else {
                vk::PipelineStageFlags::TOP_OF_PIPE
            },
            vk::PipelineStageFlags::TRANSFER,
            if initialized {
                vk::AccessFlags::SHADER_READ
            } else {
                vk::AccessFlags::empty()
            },
            vk::AccessFlags::TRANSFER_WRITE,
        );
        d.cmd_copy_buffer_to_image(
            cmd,
            source,
            self.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(self.layers),
                )
                .image_extent(vk::Extent3D {
                    width: self.size,
                    height: self.size,
                    depth: 1,
                })],
        );
        for level in 1..self.levels {
            self.barrier(
                cmd,
                level - 1,
                1,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::TRANSFER_READ,
            );
            let old = (self.size >> (level - 1)) as i32;
            let new = (self.size >> level).max(1) as i32;
            let layers = |mip| {
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(mip)
                    .layer_count(self.layers)
            };
            d.cmd_blit_image(
                cmd,
                self.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageBlit::default()
                    .src_subresource(layers(level - 1))
                    .src_offsets([
                        vk::Offset3D::default(),
                        vk::Offset3D {
                            x: old,
                            y: old,
                            z: 1,
                        },
                    ])
                    .dst_subresource(layers(level))
                    .dst_offsets([
                        vk::Offset3D::default(),
                        vk::Offset3D {
                            x: new,
                            y: new,
                            z: 1,
                        },
                    ])],
                vk::Filter::LINEAR,
            );
        }
        if self.levels > 1 {
            self.barrier(
                cmd,
                0,
                self.levels - 1,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::AccessFlags::TRANSFER_READ,
                vk::AccessFlags::SHADER_READ,
            );
        }
        self.barrier(
            cmd,
            self.levels - 1,
            1,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ,
        );
    }
}
impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
struct Frame {
    buffers: Vec<Buffer>,
    textures: Vec<Texture>,
    set: vk::DescriptorSet,
    initialized: bool,
    validated: bool,
    framebuffer: vk::Framebuffer,
}
pub struct Water {
    device: ash::Device,
    layout: vk::PipelineLayout,
    sets: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sampler: vk::Sampler,
    compute: Vec<vk::Pipeline>,
    graphics: vk::Pipeline,
    present: vk::Pipeline,
    water_pass: vk::RenderPass,
    shade_extent: vk::Extent2D,
    output_size: u32,
    shaders: Vec<vk::ShaderModule>,
    frames: Vec<Frame>,
    indices: Option<Buffer>,
    count: u32,
}
impl Water {
    pub unsafe fn new(
        d: &ash::Device,
        props: &vk::PhysicalDeviceMemoryProperties,
        pass: vk::RenderPass,
        extent: vk::Extent2D,
        slots: usize,
        shade_height: u32,
    ) -> Result<Self> {
        let mut w = Self {
            device: d.clone(),
            layout: vk::PipelineLayout::null(),
            sets: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            sampler: vk::Sampler::null(),
            compute: vec![],
            graphics: vk::Pipeline::null(),
            present: vk::Pipeline::null(),
            water_pass: vk::RenderPass::null(),
            shade_extent: vk::Extent2D {
                width: shade_height * extent.width / extent.height,
                height: shade_height,
            },
            output_size: (shade_height * extent.width / extent.height).next_power_of_two(),
            shaders: vec![],
            frames: vec![],
            indices: None,
            count: 0,
        };
        let bindings: Vec<_> = (0..10)
            .map(|i| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i)
                    .descriptor_count(1)
                    .descriptor_type(if i < 5 {
                        vk::DescriptorType::STORAGE_BUFFER
                    } else {
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER
                    })
                    .stage_flags(if i < 5 {
                        vk::ShaderStageFlags::COMPUTE
                    } else {
                        vk::ShaderStageFlags::ALL
                    })
            })
            .collect();
        w.sets = d.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        w.layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[w.sets])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::ALL)
                    .size(64)]),
            None,
        )?;
        w.pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(slots as u32)
                .pool_sizes(&[
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: slots as u32 * 5,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        descriptor_count: slots as u32 * 5,
                    },
                ]),
            None,
        )?;
        w.sampler = d.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .max_lod(10.),
            None,
        )?;
        let color = [vk::AttachmentDescription::default()
            .format(vk::Format::R8G8B8A8_UNORM)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let refs = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&refs)];
        let deps = [vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)];
        w.water_pass = d.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&color)
                .subpasses(&subpasses)
                .dependencies(&deps),
            None,
        )?;
        for bytes in [
            include_bytes!("../out/shaders/water-0.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-1.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-2.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-3.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-4.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-5.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-6.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-7.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-8.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water-9.comp.spv").as_slice(),
            include_bytes!("../out/shaders/water.vert.spv").as_slice(),
            include_bytes!("../out/shaders/water.frag.spv").as_slice(),
            include_bytes!("../out/shaders/water_present.vert.spv").as_slice(),
            include_bytes!("../out/shaders/water_present.frag.spv").as_slice(),
        ] {
            let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes))?;
            w.shaders.push(
                d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?,
            );
        }
        for mode in 0u32..10 {
            let info = vk::ComputePipelineCreateInfo::default()
                .layout(w.layout)
                .stage(
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::COMPUTE)
                        .module(w.shaders[mode as usize])
                        .name(c"main"),
                );
            match d.create_compute_pipelines(vk::PipelineCache::null(), &[info], None) {
                Ok(p) => w.compute.push(p[0]),
                Err((p, e)) => {
                    for p in p {
                        d.destroy_pipeline(p, None);
                    }
                    return Err(e.into());
                }
            }
        }
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(w.shaders[10])
                .name(c"main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(w.shaders[11])
                .name(c"main"),
        ];
        let vi = vk::PipelineVertexInputStateCreateInfo::default();
        let ia = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let views = [vk::Viewport {
            x: 0.,
            y: 0.,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.,
            max_depth: 1.,
        }];
        let rect = [vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        }];
        let vp = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&views)
            .scissors(&rect);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .line_width(1.);
        let ms = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default();
        let attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
        let dynamic = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .dynamic_state(&dynamic)
            .stages(&stages)
            .vertex_input_state(&vi)
            .input_assembly_state(&ia)
            .viewport_state(&vp)
            .rasterization_state(&raster)
            .multisample_state(&ms)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .layout(w.layout)
            .render_pass(w.water_pass);
        match d.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) {
            Ok(p) => w.graphics = p[0],
            Err((p, e)) => {
                for p in p {
                    d.destroy_pipeline(p, None);
                }
                return Err(e.into());
            }
        }
        let post_stages = [
            stages[0].module(w.shaders[12]),
            stages[1].module(w.shaders[13]),
        ];
        match d.create_graphics_pipelines(
            vk::PipelineCache::null(),
            &[info.stages(&post_stages).render_pass(pass)],
            None,
        ) {
            Ok(p) => w.present = p[0],
            Err((p, e)) => {
                for p in p {
                    d.destroy_pipeline(p, None);
                }
                return Err(e.into());
            }
        }
        for _ in 0..slots {
            let mut frame = Frame {
                buffers: vec![],
                textures: vec![],
                set: vk::DescriptorSet::null(),
                initialized: false,
                validated: false,
                framebuffer: vk::Framebuffer::null(),
            };
            let color = [vk::AttachmentDescription::default()
                .format(vk::Format::R8G8B8A8_UNORM)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let refs = [vk::AttachmentReference::default()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
            let subpasses = [vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&refs)];
            let deps = [vk::SubpassDependency::default()
                .src_subpass(0)
                .dst_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)];
            w.water_pass = d.create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&color)
                    .subpasses(&subpasses)
                    .dependencies(&deps),
                None,
            )?;
            for bytes in [32768 * 16, 32768 * 8, 32768 * 8, 65536 * 4, 1024 * 1024 * 4] {
                frame.buffers.push(Buffer::new(
                    d,
                    props,
                    bytes,
                    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
                )?);
            }
            frame.textures.push(Texture::new(d, props, 128, 2, 8)?);
            frame.textures.push(Texture::new_format(
                d,
                props,
                256,
                1,
                9,
                vk::Format::R16_SFLOAT,
                false,
            )?);
            frame.textures.push(Texture::new_format(
                d,
                props,
                1024,
                1,
                11,
                vk::Format::R8G8B8A8_UNORM,
                false,
            )?);
            frame.textures.push(Texture::new_format(
                d,
                props,
                512,
                1,
                1,
                vk::Format::B10G11R11_UFLOAT_PACK32,
                false,
            )?);
            frame.textures.push(Texture::new_format(
                d,
                props,
                w.output_size,
                1,
                1,
                vk::Format::R8G8B8A8_UNORM,
                true,
            )?);
            frame.framebuffer = d.create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(w.water_pass)
                    .attachments(&[frame.textures[4].view])
                    .width(w.output_size)
                    .height(w.output_size)
                    .layers(1),
                None,
            )?;
            frame.set = d.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(w.pool)
                    .set_layouts(&[w.sets]),
            )?[0];
            for (i, b) in frame.buffers.iter().enumerate() {
                let info = [vk::DescriptorBufferInfo::default()
                    .buffer(b.handle)
                    .range(vk::WHOLE_SIZE)];
                d.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(frame.set)
                        .dst_binding(i as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&info)],
                    &[],
                );
            }
            for (i, t) in frame.textures.iter().enumerate() {
                let info = [vk::DescriptorImageInfo::default()
                    .sampler(w.sampler)
                    .image_view(t.view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                d.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(frame.set)
                        .dst_binding(5 + i as u32)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&info)],
                    &[],
                );
            }
            w.frames.push(frame);
        }
        let mut indices = Vec::<u16>::with_capacity(64 * 36 * 6);
        for y in 0..36u16 {
            for x in 0..64u16 {
                let a = y * 65 + x;
                indices.extend_from_slice(&[a, a + 65, a + 1, a + 1, a + 65, a + 66]);
            }
        }
        w.count = indices.len() as u32;
        let ib = Buffer::new(
            d,
            props,
            indices.len() * 2,
            vk::BufferUsageFlags::INDEX_BUFFER,
        )?;
        ib.write(&indices)?;
        w.indices = Some(ib);
        crate::log::log("CLEARWATER FFT=2x128 photonRays=256 caustics=256 bed=1024 projectedGrid=64x36 continuous simulation; independent water shading resolution");
        crate::log::log(&format!(
            "CLEARWATER shading={}x{} presentation={}x{} simulation=continuous",
            w.shade_extent.width, w.shade_extent.height, extent.width, extent.height
        ));
        Ok(w)
    }
    unsafe fn memory_barrier(&self, cmd: vk::CommandBuffer) {
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COMPUTE_SHADER
                | vk::PipelineStageFlags::TRANSFER
                | vk::PipelineStageFlags::VERTEX_SHADER,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(
                    vk::AccessFlags::SHADER_WRITE
                        | vk::AccessFlags::TRANSFER_WRITE
                        | vk::AccessFlags::TRANSFER_READ,
                )
                .dst_access_mask(
                    vk::AccessFlags::SHADER_READ
                        | vk::AccessFlags::SHADER_WRITE
                        | vk::AccessFlags::TRANSFER_READ
                        | vk::AccessFlags::TRANSFER_WRITE,
                )],
            &[],
            &[],
        );
    }
    unsafe fn dispatch(&self, cmd: vk::CommandBuffer, mode: usize, x: u32, y: u32) {
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.compute[mode]);
        self.device.cmd_dispatch(cmd, x, y, 1);
        self.memory_barrier(cmd);
    }
    /// One startup sanity check per slot, outside measured steady-state rendering.
    pub unsafe fn validate_completed(&mut self, slot: usize) -> Result<()> {
        let f = &mut self.frames[slot];
        if !f.initialized || f.validated {
            return Ok(());
        }
        let fft = f.buffers[1].read::<[f32; 2]>(32768)?;
        let photons = f.buffers[3].read::<u32>(65536)?;
        let mut residual = 0f32;
        let mut rms = 0f64;
        for v in &fft {
            if !v[0].is_finite() || !v[1].is_finite() {
                return Err("Non-finite FFT water".into());
            }
            residual = residual.max(v[1].abs());
            rms += (v[0] as f64).powi(2);
        }
        let mean = photons.iter().map(|v| *v as f64).sum::<f64>() / (65536. * 4096.);
        if residual > 0.0001 || (mean - 1.).abs() > 0.002 {
            return Err(format!(
                "Water validation failed: FFT imaginary={residual} photon mean={mean}"
            )
            .into());
        }
        crate::log::log(&format!("CLEARWATER VALID slot={slot} heightRms={:.6} imaginary={residual:.8} photonMean={mean:.6}",(rms/32768.).sqrt()));
        f.validated = true;
        Ok(())
    }
    pub unsafe fn prepare(&mut self, cmd: vk::CommandBuffer, slot: usize, camera: &WaterCamera) {
        let f = &self.frames[slot];
        let bytes = std::slice::from_raw_parts((camera as *const WaterCamera).cast::<u8>(), 64);
        self.device
            .cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::ALL, 0, bytes);
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.layout,
            0,
            &[f.set],
            &[],
        );
        if !f.initialized {
            self.dispatch(cmd, 0, 512, 1);
            self.dispatch(cmd, 9, 4096, 1);
            f.textures[3].upload(cmd, f.buffers[4].handle, false);
            self.memory_barrier(cmd);
            self.dispatch(cmd, 8, 16384, 1);
            f.textures[2].upload(cmd, f.buffers[4].handle, false);
            self.memory_barrier(cmd);
        }
        self.dispatch(cmd, 1, 512, 1);
        self.dispatch(cmd, 2, 128, 2);
        self.dispatch(cmd, 3, 128, 2);
        self.dispatch(cmd, 4, 512, 1);
        f.textures[0].upload(cmd, f.buffers[4].handle, f.initialized);
        self.memory_barrier(cmd);
        self.dispatch(cmd, 5, 1024, 1);
        self.dispatch(cmd, 6, 1024, 1);
        self.dispatch(cmd, 7, 512, 1);
        f.textures[1].upload(cmd, f.buffers[4].handle, f.initialized);
        self.memory_barrier(cmd);
        if !self.frames[slot].initialized {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)],
                &[],
                &[],
            );
        }
        self.frames[slot].initialized = true;
        let rect = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: self.shade_extent,
        };
        self.device.cmd_begin_render_pass(
            cmd,
            &vk::RenderPassBeginInfo::default()
                .render_pass(self.water_pass)
                .framebuffer(self.frames[slot].framebuffer)
                .render_area(rect),
            vk::SubpassContents::INLINE,
        );
        self.draw_water(cmd, slot, camera);
        self.device.cmd_end_render_pass(cmd);
    }
    unsafe fn viewport(&self, cmd: vk::CommandBuffer, extent: vk::Extent2D) {
        self.device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.,
                y: 0.,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.,
                max_depth: 1.,
            }],
        );
        self.device.cmd_set_scissor(
            cmd,
            0,
            &[vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent,
            }],
        );
    }
    pub unsafe fn draw(&self, cmd: vk::CommandBuffer, slot: usize, extent: vk::Extent2D) {
        self.viewport(cmd, extent);
        let scale = [
            self.shade_extent.width as f32 / self.output_size as f32,
            self.shade_extent.height as f32 / self.output_size as f32,
        ];
        let bytes = std::slice::from_raw_parts(scale.as_ptr().cast::<u8>(), 8);
        self.device
            .cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::ALL, 0, bytes);
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.present);
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            self.layout,
            0,
            &[self.frames[slot].set],
            &[],
        );
        self.device.cmd_draw(cmd, 3, 1, 0, 0);
    }
    unsafe fn draw_water(&self, cmd: vk::CommandBuffer, slot: usize, camera: &WaterCamera) {
        self.viewport(cmd, self.shade_extent);
        let d = &self.device;
        let bytes = std::slice::from_raw_parts((camera as *const WaterCamera).cast::<u8>(), 64);
        d.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::ALL, 0, bytes);
        d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.graphics);
        d.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            self.layout,
            0,
            &[self.frames[slot].set],
            &[],
        );
        d.cmd_bind_index_buffer(
            cmd,
            self.indices.as_ref().unwrap().handle,
            0,
            vk::IndexType::UINT16,
        );
        d.cmd_draw_indexed(cmd, self.count, 1, 0, 0, 0);
    }
}
impl Drop for Water {
    fn drop(&mut self) {
        unsafe {
            let d = &self.device;
            let _ = d.device_wait_idle();
            d.destroy_descriptor_pool(self.pool, None);
            for f in &self.frames {
                d.destroy_framebuffer(f.framebuffer, None);
            }
            self.frames.clear();
            self.indices.take();
            d.destroy_pipeline(self.graphics, None);
            d.destroy_pipeline(self.present, None);
            d.destroy_render_pass(self.water_pass, None);
            for p in &self.compute {
                d.destroy_pipeline(*p, None);
            }
            d.destroy_pipeline_layout(self.layout, None);
            d.destroy_descriptor_set_layout(self.sets, None);
            d.destroy_sampler(self.sampler, None);
            for s in &self.shaders {
                d.destroy_shader_module(*s, None);
            }
        }
    }
}
