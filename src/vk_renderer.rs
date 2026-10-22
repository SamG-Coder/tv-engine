//! Small opaque forward renderer: arbitrary indexed meshes and matrix/tint instances.
//! GPU ownership stays on the render thread; callers wait the frame fence before updating.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
use crate::geometry::Vertex;
use ash::vk;
use std::{ffi::c_void, mem::size_of, ptr};
pub struct Buffer {
    device: ash::Device,
    pub handle: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *mut c_void,
    bytes: usize,
    coherent: bool,
}
impl Buffer {
    pub unsafe fn new(
        d: &ash::Device,
        props: &vk::PhysicalDeviceMemoryProperties,
        bytes: usize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let mut b = Self {
            device: d.clone(),
            handle: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
            mapped: ptr::null_mut(),
            bytes,
            coherent: false,
        };
        b.handle = d.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(bytes.max(4) as u64)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        )?;
        let req = d.get_buffer_memory_requirements(b.handle);
        let flags = vk::MemoryPropertyFlags::HOST_VISIBLE;
        let ty = (0..props.memory_type_count)
            .filter(|i| {
                req.memory_type_bits & (1 << i) != 0
                    && props.memory_types[*i as usize]
                        .property_flags
                        .contains(flags)
            })
            .max_by_key(|i| {
                let flags = props.memory_types[*i as usize].property_flags;
                (flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT) as u32) * 2
                    + flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) as u32
            })
            .ok_or("No host-visible memory")?;
        b.coherent = props.memory_types[ty as usize]
            .property_flags
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);
        crate::log::log(&format!(
            "VULKAN buffer bytes={bytes} memoryType={ty} flags={:?}",
            props.memory_types[ty as usize].property_flags
        ));
        b.memory = d.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(ty),
            None,
        )?;
        d.bind_buffer_memory(b.handle, b.memory, 0)?;
        b.mapped = d.map_memory(b.memory, 0, req.size, vk::MemoryMapFlags::empty())?;
        Ok(b)
    }
    /// Diagnostic readback only, after the submitting fence and a GPU-to-host barrier.
    pub unsafe fn read<T: Copy + Default>(&self, count: usize) -> Result<Vec<T>> {
        assert!(count * size_of::<T>() <= self.bytes);
        if !self.coherent {
            self.device
                .invalidate_mapped_memory_ranges(&[vk::MappedMemoryRange::default()
                    .memory(self.memory)
                    .size(vk::WHOLE_SIZE)])?;
        }
        let mut result = vec![T::default(); count];
        ptr::copy_nonoverlapping(self.mapped.cast::<T>(), result.as_mut_ptr(), count);
        Ok(result)
    }
    pub unsafe fn write<T: Copy>(&self, data: &[T]) -> Result<()> {
        assert!(std::mem::size_of_val(data) <= self.bytes);
        ptr::copy_nonoverlapping(
            data.as_ptr().cast::<u8>(),
            self.mapped.cast(),
            std::mem::size_of_val(data),
        );
        if !self.coherent && !data.is_empty() {
            self.device
                .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::default()
                    .memory(self.memory)
                    .offset(0)
                    .size(vk::WHOLE_SIZE)])?;
        }
        Ok(())
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            if !self.mapped.is_null() {
                self.device.unmap_memory(self.memory);
            }
            self.device.destroy_buffer(self.handle, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
pub struct Depth {
    device: ash::Device,
    pub image: vk::Image,
    pub view: vk::ImageView,
    memory: vk::DeviceMemory,
}
impl Depth {
    pub unsafe fn new(
        d: &ash::Device,
        props: &vk::PhysicalDeviceMemoryProperties,
        extent: vk::Extent2D,
    ) -> Result<Self> {
        let mut x = Self {
            device: d.clone(),
            image: vk::Image::null(),
            view: vk::ImageView::null(),
            memory: vk::DeviceMemory::null(),
        };
        x.image = d.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::D16_UNORM)
                .extent(vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT,
                ),
            None,
        )?;
        let req = d.get_image_memory_requirements(x.image);
        let ty = (0..props.memory_type_count)
            .filter(|i| {
                req.memory_type_bits & (1 << i) != 0
                    && props.memory_types[*i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            })
            .max_by_key(|i| {
                props.memory_types[*i as usize]
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::LAZILY_ALLOCATED)
            })
            .ok_or("No depth memory")?;
        crate::log::log(&format!(
            "VULKAN depth memoryType={ty} flags={:?} allocation={} bytes",
            props.memory_types[ty as usize].property_flags, req.size
        ));
        x.memory = d.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(ty),
            None,
        )?;
        d.bind_image_memory(x.image, x.memory, 0)?;
        x.view = d.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(x.image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::D16_UNORM)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .level_count(1)
                        .layer_count(1),
                ),
            None,
        )?;
        Ok(x)
    }
}
impl Drop for Depth {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
/// Owns a deduplicated vertex/index buffer. Ranges preserve original triangle-list offsets.
pub struct Mesh {
    vertices: Buffer,
    indices: Buffer,
    pub count: u32,
    index_type: vk::IndexType,
}
impl Mesh {
    pub unsafe fn upload(
        d: &ash::Device,
        props: &vk::PhysicalDeviceMemoryProperties,
        vertices: &[Vertex],
    ) -> Result<Self> {
        let mut unique = Vec::new();
        let mut indices = Vec::with_capacity(vertices.len());
        let mut lookup = std::collections::HashMap::new();
        for v in vertices {
            let key = [
                v.p[0].to_bits(),
                v.p[1].to_bits(),
                v.p[2].to_bits(),
                v.c[0].to_bits(),
                v.c[1].to_bits(),
                v.c[2].to_bits(),
            ];
            let id = *lookup.entry(key).or_insert_with(|| {
                let i = unique.len() as u32;
                unique.push(*v);
                i
            });
            indices.push(id);
        }
        let vb = Buffer::new(
            d,
            props,
            unique.len() * size_of::<Vertex>(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        vb.write(&unique)?;
        let (ib, index_type) = if unique.len() <= u16::MAX as usize + 1 {
            let indices16: Vec<u16> = indices.iter().map(|i| *i as u16).collect();
            let buffer = Buffer::new(
                d,
                props,
                indices16.len() * 2,
                vk::BufferUsageFlags::INDEX_BUFFER,
            )?;
            buffer.write(&indices16)?;
            (buffer, vk::IndexType::UINT16)
        } else {
            let buffer = Buffer::new(
                d,
                props,
                indices.len() * 4,
                vk::BufferUsageFlags::INDEX_BUFFER,
            )?;
            buffer.write(&indices)?;
            (buffer, vk::IndexType::UINT32)
        };
        Ok(Self {
            vertices: vb,
            indices: ib,
            count: indices.len() as u32,
            index_type,
        })
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Camera {
    pub vp: [f32; 16],
    pub eye_time: [f32; 4],
}
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum Material {
    Static = 0,
    Water = 1,
    Bobbing = 2,
    Instanced = 3,
}
pub struct Renderer {
    device: ash::Device,
    pub layout: vk::PipelineLayout,
    pipeline: [vk::Pipeline; 4],
    bound: std::cell::Cell<u32>,
    bound_mesh: std::cell::Cell<(vk::Buffer, vk::Buffer, vk::Buffer, u64)>,
    shaders: Vec<vk::ShaderModule>,
}
impl Renderer {
    pub unsafe fn new(d: &ash::Device, pass: vk::RenderPass, extent: vk::Extent2D) -> Result<Self> {
        let mut r = Self {
            device: d.clone(),
            layout: vk::PipelineLayout::null(),
            pipeline: [vk::Pipeline::null(); 4],
            bound: std::cell::Cell::new(u32::MAX),
            bound_mesh: std::cell::Cell::new((
                vk::Buffer::null(),
                vk::Buffer::null(),
                vk::Buffer::null(),
                u64::MAX,
            )),
            shaders: vec![],
        };
        for bytes in [
            include_bytes!("../out/shaders/scene.vert.spv").as_slice(),
            include_bytes!("../out/shaders/scene.frag.spv").as_slice(),
        ] {
            let words = ash::util::read_spv(&mut std::io::Cursor::new(bytes))?;
            r.shaders.push(
                d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)?,
            );
        }
        let ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(size_of::<Camera>() as u32)];
        r.layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&ranges),
            None,
        )?;
        let name = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(r.shaders[0])
                .name(name),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(r.shaders[1])
                .name(name),
        ];
        let bindings = [
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: 24,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: 80,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ];
        let mut attrs = vec![
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 12,
            },
        ];
        for i in 0..5 {
            attrs.push(vk::VertexInputAttributeDescription {
                location: 2 + i,
                binding: 1,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: 16 * i,
            });
        }
        let vi = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attrs);
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
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        }];
        let viewport = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&views)
            .scissors(&rect);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.);
        let samples = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS);
        let blend = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blending = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vi)
            .input_assembly_state(&ia)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&samples)
            .depth_stencil_state(&depth)
            .color_blend_state(&blending)
            .layout(r.layout)
            .render_pass(pass);
        for mode in 0u32..4 {
            let entries = [vk::SpecializationMapEntry {
                constant_id: 0,
                offset: 0,
                size: 4,
            }];
            let data = mode.to_ne_bytes();
            let specialization = vk::SpecializationInfo::default()
                .map_entries(&entries)
                .data(&data);
            let variant = [stages[0].specialization_info(&specialization), stages[1]];
            match d.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[info.stages(&variant)],
                None,
            ) {
                Ok(p) => r.pipeline[mode as usize] = p[0],
                Err((partial, e)) => {
                    for p in partial {
                        d.destroy_pipeline(p, None);
                    }
                    return Err(e.into());
                }
            }
        }
        Ok(r)
    }
    /// Begin an opaque pass. Camera constants are uploaded once for all draws in this pass.
    pub unsafe fn begin(&self, cmd: vk::CommandBuffer, camera: &Camera) {
        self.bound.set(u32::MAX);
        self.bound_mesh.set((
            vk::Buffer::null(),
            vk::Buffer::null(),
            vk::Buffer::null(),
            u64::MAX,
        ));
        let bytes =
            std::slice::from_raw_parts((camera as *const Camera).cast::<u8>(), size_of::<Camera>());
        self.device
            .cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::VERTEX, 0, bytes);
    }
    pub unsafe fn draw(
        &self,
        cmd: vk::CommandBuffer,
        mesh: &Mesh,
        instances: &Buffer,
        first_instance: u32,
        count: u32,
        first_index: u32,
        index_count: u32,
        material: Material,
    ) {
        let mode = material as u32;
        if self.bound.get() != mode {
            self.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline[mode as usize],
            );
            self.bound.set(mode);
        }
        if count == 0 {
            return;
        }
        let key = (
            mesh.vertices.handle,
            mesh.indices.handle,
            instances.handle,
            first_instance as u64 * 80,
        );
        if self.bound_mesh.get() != key {
            self.device
                .cmd_bind_vertex_buffers(cmd, 0, &[key.0, key.2], &[0, key.3]);
            self.device
                .cmd_bind_index_buffer(cmd, key.1, 0, mesh.index_type);
            self.bound_mesh.set(key);
        }
        self.device
            .cmd_draw_indexed(cmd, index_count, count, first_index, 0, 0);
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            for p in self.pipeline {
                self.device.destroy_pipeline(p, None);
            }
            self.device.destroy_pipeline_layout(self.layout, None);
            for s in &self.shaders {
                self.device.destroy_shader_module(*s, None);
            }
        }
    }
}
