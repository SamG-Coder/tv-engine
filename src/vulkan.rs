//! Android presentation host for the reusable Vulkan opaque renderer.
use crate::vk_renderer::{Buffer, Camera, Depth, Material, Mesh, Renderer};
use crate::{geometry::Instance, math::*, scene};
use ash::{vk, Entry};
use std::{
    ffi::{c_void, CStr},
    sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
    time::Instant,
};
const FRAMES_IN_FLIGHT: usize = 2;
static KEYS: AtomicI32 = AtomicI32::new(0);
static TURN: AtomicU32 = AtomicU32::new(0);
static MOVE: AtomicU32 = AtomicU32::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
#[link(name = "android")]
extern "C" {
    fn ANativeWindow_fromSurface(env: *mut c_void, surface: *mut c_void) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
}
fn log(s: &str) {
    crate::log::log(s);
}
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Resources {
    _entry: Entry,
    instance: ash::Instance,
    surface_api: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    device: Option<ash::Device>,
    swap_api: Option<ash::khr::swapchain::Device>,
    swap: vk::SwapchainKHR,
    views: Vec<vk::ImageView>,
    frames: Vec<vk::Framebuffer>,
    pass: vk::RenderPass,
    pool: vk::CommandPool,
    acquired: Vec<vk::Semaphore>,
    finished: Vec<vk::Semaphore>,
    fence: Vec<vk::Fence>,
    depth: Option<Depth>,
}
impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            if let Some(d) = &self.device {
                let _ = d.device_wait_idle();
                for f in &self.frames {
                    d.destroy_framebuffer(*f, None);
                }
                for v in &self.views {
                    d.destroy_image_view(*v, None);
                }
                self.depth.take();
                d.destroy_render_pass(self.pass, None);
                d.destroy_command_pool(self.pool, None);
                for semaphore in &self.acquired {
                    d.destroy_semaphore(*semaphore, None);
                }
                for s in &self.finished {
                    d.destroy_semaphore(*s, None);
                }
                for fence in &self.fence {
                    d.destroy_fence(*fence, None);
                }
                if let Some(s) = &self.swap_api {
                    s.destroy_swapchain(self.swap, None);
                }
                d.destroy_device(None);
            }
            self.surface_api.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}
unsafe fn run(
    window: *mut c_void,
    objects: usize,
    limit: u32,
    tour: bool,
    fixed_time: f32,
    water_mode: bool,
    water_height: u32,
) -> Result<()> {
    let entry = Entry::load()?;
    let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_0);
    let extensions = [
        ash::khr::surface::NAME.as_ptr(),
        ash::khr::android_surface::NAME.as_ptr(),
    ];
    let instance = entry.create_instance(
        &vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&extensions),
        None,
    )?;
    let surface_api = ash::khr::surface::Instance::new(&entry, &instance);
    let android = ash::khr::android_surface::Instance::new(&entry, &instance);
    let mut r = Resources {
        _entry: entry,
        instance,
        surface_api,
        surface: vk::SurfaceKHR::null(),
        device: None,
        swap_api: None,
        swap: vk::SwapchainKHR::null(),
        views: vec![],
        frames: vec![],
        pass: vk::RenderPass::null(),
        pool: vk::CommandPool::null(),
        acquired: vec![],
        finished: vec![],
        fence: vec![],
        depth: None,
    };
    r.surface = android.create_android_surface(
        &vk::AndroidSurfaceCreateInfoKHR::default().window(window.cast()),
        None,
    )?;
    let mut selected = None;
    for physical in r.instance.enumerate_physical_devices()? {
        let p = r.instance.get_physical_device_properties(physical);
        log(&format!(
            "VULKAN device={} API={}.{}.{} driver={} type={:?} timestampPeriod={}ns",
            CStr::from_ptr(p.device_name.as_ptr()).to_string_lossy(),
            vk::api_version_major(p.api_version),
            vk::api_version_minor(p.api_version),
            vk::api_version_patch(p.api_version),
            p.driver_version,
            p.device_type,
            p.limits.timestamp_period
        ));
        for (i, q) in r
            .instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .enumerate()
        {
            if q.queue_flags
                .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
                && r.surface_api
                    .get_physical_device_surface_support(physical, i as u32, r.surface)?
            {
                selected = Some((physical, i as u32));
                log(&format!(
                    "VULKAN graphics+present queue={i} timestampValidBits={}",
                    q.timestamp_valid_bits
                ));
                break;
            }
        }
        if selected.is_some() {
            break;
        }
    }
    let (physical, family) = selected.ok_or("No graphics/present queue")?;
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(family)
        .queue_priorities(&priorities)];
    let dext = [ash::khr::swapchain::NAME.as_ptr()];
    r.device = Some(
        r.instance.create_device(
            physical,
            &vk::DeviceCreateInfo::default()
                .queue_create_infos(&queues)
                .enabled_extension_names(&dext),
            None,
        )?,
    );
    let d = r.device.as_ref().unwrap();
    let queue = d.get_device_queue(family, 0);
    let swap_api = ash::khr::swapchain::Device::new(&r.instance, d);
    let caps = r
        .surface_api
        .get_physical_device_surface_capabilities(physical, r.surface)?;
    let formats = r
        .surface_api
        .get_physical_device_surface_formats(physical, r.surface)?;
    let format = *formats
        .iter()
        .find(|f| f.format == vk::Format::R8G8B8A8_UNORM)
        .or(formats.first())
        .ok_or("No surface formats")?;
    let extent = if caps.current_extent.width == u32::MAX {
        vk::Extent2D {
            width: 3840u32.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
            height: 2160u32.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
        }
    } else {
        caps.current_extent
    };
    let mut count = caps.min_image_count.max(3);
    if caps.max_image_count > 0 {
        count = count.min(caps.max_image_count);
    }
    let alpha = [
        vk::CompositeAlphaFlagsKHR::OPAQUE,
        vk::CompositeAlphaFlagsKHR::INHERIT,
        vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
    ]
    .into_iter()
    .find(|a| caps.supported_composite_alpha.contains(*a))
    .ok_or("No composite alpha")?;
    r.swap_api = Some(swap_api);
    let swap_api = r.swap_api.as_ref().unwrap();
    r.swap = swap_api.create_swapchain(
        &vk::SwapchainCreateInfoKHR::default()
            .surface(r.surface)
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true),
        None,
    )?;
    let images = swap_api.get_swapchain_images(r.swap)?;
    log(&format!(
        "VULKAN swapchain={}x{} format={:?} images={} FIFO",
        extent.width,
        extent.height,
        format.format,
        images.len()
    ));
    let props = r.instance.get_physical_device_memory_properties(physical);
    for i in 0..props.memory_type_count {
        log(&format!(
            "VULKAN memoryType={i} flags={:?} heap={}",
            props.memory_types[i as usize].property_flags,
            props.memory_types[i as usize].heap_index
        ));
    }
    let features = r.instance.get_physical_device_features(physical);
    log(&format!("VULKAN features multiDrawIndirect={} firstInstance={} int16={} ASTC={} ETC2={} pipelineStats={}",features.multi_draw_indirect,features.draw_indirect_first_instance,features.shader_int16,features.texture_compression_astc_ldr,features.texture_compression_etc2,features.pipeline_statistics_query));
    r.depth = Some(Depth::new(d, &props, extent)?);
    let attachments = [
        vk::AttachmentDescription::default()
            .format(format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR),
        vk::AttachmentDescription::default()
            .format(vk::Format::D16_UNORM)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
    ];
    let refs = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let depth_ref = vk::AttachmentReference::default()
        .attachment(1)
        .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&refs)
        .depth_stencil_attachment(&depth_ref)];
    let dependencies = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        )
        .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
        .dst_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        )
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        )];
    r.pass = d.create_render_pass(
        &vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies),
        None,
    )?;
    for &img in &images {
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let view = d.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(img)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.format)
                .subresource_range(range),
            None,
        )?;
        r.views.push(view);
        r.frames.push(
            d.create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(r.pass)
                    .attachments(&[view, r.depth.as_ref().unwrap().view])
                    .width(extent.width)
                    .height(extent.height)
                    .layers(1),
                None,
            )?,
        );
        r.finished
            .push(d.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?);
    }
    r.pool = d.create_command_pool(
        &vk::CommandPoolCreateInfo::default()
            .queue_family_index(family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        None,
    )?;
    let commands = d.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(r.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(FRAMES_IN_FLIGHT as u32),
    )?;
    for _ in 0..FRAMES_IN_FLIGHT {
        r.acquired
            .push(d.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?);
        r.fence.push(d.create_fence(
            &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
            None,
        )?);
    }
    if water_mode {
        let features = r
            .instance
            .get_physical_device_format_properties(physical, vk::Format::R16G16B16A16_SFLOAT)
            .optimal_tiling_features;
        if !features.contains(
            vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                | vk::FormatFeatureFlags::BLIT_SRC
                | vk::FormatFeatureFlags::BLIT_DST,
        ) {
            return Err("Water requires filterable/blittable RGBA16F textures".into());
        }
    }
    if water_mode {
        for format in [
            vk::Format::R16_SFLOAT,
            vk::Format::B10G11R11_UFLOAT_PACK32,
            vk::Format::R8G8B8A8_UNORM,
        ] {
            let flags = r
                .instance
                .get_physical_device_format_properties(physical, format)
                .optimal_tiling_features;
            if !flags.contains(
                vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                    | vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST,
            ) {
                return Err(format!("Water format unsupported: {format:?}").into());
            }
        }
    }
    let mut water = if water_mode {
        Some(crate::water::Water::new(
            d,
            &props,
            r.pass,
            extent,
            FRAMES_IN_FLIGHT,
            water_height,
        )?)
    } else {
        None
    };
    let pipeline = Renderer::new(d, r.pass, extent)?;
    let mut scene = scene::build();
    let terrain = Mesh::upload(d, &props, &scene.vertices)?;
    scene.vertices.clear();
    scene.vertices.shrink_to_fit();
    let (vertices, objects) = scene::stress(objects);
    let stress = Mesh::upload(d, &props, &vertices)?;
    drop(vertices);
    let identity = Instance {
        model: [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ],
        tint: [1.; 4],
    };
    let mut gems = vec![identity; 13];
    for (i, p) in scene.gems.iter().enumerate() {
        gems[i + 1].model[12] = p.0;
        gems[i + 1].model[13] = p.1;
        gems[i + 1].model[14] = p.2;
    }
    let mut fixed = Vec::with_capacity(FRAMES_IN_FLIGHT);
    let mut dynamic = Vec::with_capacity(FRAMES_IN_FLIGHT);
    for _ in 0..FRAMES_IN_FLIGHT {
        let buffer = Buffer::new(
            d,
            &props,
            gems.len() * 80,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        buffer.write(&gems)?;
        fixed.push(buffer);
        dynamic.push(Buffer::new(
            d,
            &props,
            objects.len().max(1) * 80,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?);
    }
    let mut visibility = crate::visibility::Visibility::new(&objects, 3f32.sqrt());
    drop(objects);
    let started = Instant::now();
    let mut window = Instant::now();
    let mut totals = [0f64; 5];
    let mut frames = 0;
    let mut scene_time = 0f32;
    let mut eye = V(0., 0., 21.);
    let mut yaw = 0f32;
    let mut found = [false; 12];
    let mut previous = Instant::now();
    let mut rendered = 0;
    while (limit == 0 || rendered < limit) && !STOP.load(Ordering::Relaxed) {
        let slot = rendered as usize % FRAMES_IN_FLIGHT;
        let cmd = commands[slot];
        let dynamic = &dynamic[slot];
        let fixed = &fixed[slot];
        let begin = Instant::now();
        d.wait_for_fences(&[r.fence[slot]], true, 2_000_000_000)?;
        if let Some(w) = &mut water {
            w.validate_completed(slot)?;
        }
        let waited = begin.elapsed().as_secs_f64();
        let acquire = Instant::now();
        let (idx, _) = swap_api.acquire_next_image(
            r.swap,
            2_000_000_000,
            r.acquired[slot],
            vk::Fence::null(),
        )?;
        let acquired = acquire.elapsed().as_secs_f64();
        let prepare = Instant::now();
        let now = Instant::now();
        let dt = (now - previous).as_secs_f32().min(0.05);
        previous = now;
        scene_time = if water_mode {
            started.elapsed().as_secs_f32()
        } else if fixed_time >= 0. {
            fixed_time
        } else {
            scene_time + dt
        };
        let keys = KEYS.load(Ordering::Relaxed);
        yaw += (f32::from_bits(TURN.load(Ordering::Relaxed)) + if keys & 8 != 0 { 1. } else { 0. }
            - if keys & 4 != 0 { 1. } else { 0. })
            * dt
            * 1.65;
        let forward = V(
            yaw.sin(),
            if water_mode { -0.45 } else { -0.12 },
            -yaw.cos(),
        )
        .norm();
        let walk = f32::from_bits(MOVE.load(Ordering::Relaxed))
            + if keys & 1 != 0 { 1. } else { 0. }
            - if keys & 2 != 0 { 1. } else { 0. };
        let next =
            eye + V(forward.0, 0., forward.2) * (walk * dt * if keys & 16 != 0 { 9. } else { 4. });
        if next.0 * next.0 + next.2 * next.2 < 31. * 31. {
            eye = next;
        }
        if tour {
            let a = scene_time * 0.16;
            eye = V(a.sin() * 19., 0., a.cos() * 19.);
            yaw = -a;
        }
        eye.1 = if water_mode {
            1.65
        } else {
            scene::height(eye.0, eye.2) + 2.2
        };
        let forward = V(
            yaw.sin(),
            if water_mode { -0.45 } else { -0.12 },
            -yaw.cos(),
        )
        .norm();
        if !tour {
            for (i, p) in scene.gems.iter().enumerate() {
                let delta = *p - eye;
                if delta.dot(delta) < 3.5 {
                    found[i] = true;
                }
            }
        }
        let mut gem_instances = [identity; 13];
        let mut gem_count = 0;
        for (i, gem) in gems.iter().skip(1).enumerate() {
            if !found[i] {
                gem_count += 1;
                gem_instances[gem_count] = *gem;
            }
        }
        fixed.write(&gem_instances[..gem_count + 1])?;
        let vp = camera(eye, forward, extent.width as f32 / extent.height as f32);
        visibility.update(vp, eye);
        let visible = &visibility.instances;
        dynamic.write(&visible)?;
        let camera = Camera {
            vp,
            eye_time: [eye.0, eye.1, eye.2, scene_time],
        };
        let culled = prepare.elapsed().as_secs_f64();
        d.reset_fences(&[r.fence[slot]])?;
        d.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        d.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        let right = forward.cross(V(0., 1., 0.)).norm();
        let up = right.cross(forward).norm();
        let water_camera = crate::water::WaterCamera {
            eye_time: camera.eye_time,
            forward_aspect: [
                forward.0,
                forward.1,
                forward.2,
                extent.width as f32 / extent.height as f32,
            ],
            right_depth: [right.0, right.1, right.2, 1.4],
            up_exposure: [up.0, up.1, up.2, 1.],
        };
        if let Some(w) = &mut water {
            w.prepare(cmd, slot, &water_camera);
        }
        let clear = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.57, 0.74, 0.78, 1.],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.,
                    stencil: 0,
                },
            },
        ];
        let rect = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
        d.cmd_begin_render_pass(
            cmd,
            &vk::RenderPassBeginInfo::default()
                .render_pass(r.pass)
                .framebuffer(r.frames[idx as usize])
                .render_area(rect)
                .clear_values(&clear),
            vk::SubpassContents::INLINE,
        );
        let mut draws = 0;
        let mut triangles = 0;
        if let Some(w) = &water {
            w.draw(cmd, slot, extent);
            draws = 1;
            triangles = 64 * 36 * 2;
        } else {
            pipeline.begin(cmd, &camera);
            let mut material = Material::Static;
            let mut order: [usize; 64] = std::array::from_fn(|i| i);
            order.sort_unstable_by(|a, b| {
                let a = scene.chunks[*a].center - eye;
                let b = scene.chunks[*b].center - eye;
                a.dot(a).total_cmp(&b.dot(b))
            });
            for i in order {
                let chunk = &scene.chunks[i];
                let delta = chunk.center - eye;
                let dist = delta.dot(delta).sqrt();
                if dist > 85. || (dist > 12. && delta.dot(forward) < dist * 0.25 - 12.) {
                    continue;
                }
                pipeline.draw(
                    cmd,
                    &terrain,
                    &fixed,
                    0,
                    1,
                    chunk.first as u32,
                    chunk.count as u32,
                    material,
                );
                draws += 1;
                triangles += chunk.count / 3;
            }
            material = Material::Water;
            pipeline.draw(
                cmd,
                &terrain,
                &fixed,
                0,
                1,
                scene.water.0 as u32,
                scene.water.1 as u32,
                material,
            );
            draws += 1;
            triangles += scene.water.1 / 3;
            material = Material::Bobbing;
            pipeline.draw(
                cmd,
                &terrain,
                &fixed,
                1,
                gem_count as u32,
                scene.gem.0 as u32,
                scene.gem.1 as u32,
                material,
            );
            draws += 1;
            triangles += scene.gem.1 / 3 * gem_count as i32;
            material = Material::Instanced;
            pipeline.draw(
                cmd,
                &stress,
                &dynamic,
                0,
                visible.len() as u32,
                0,
                stress.count,
                material,
            );
            if !visible.is_empty() {
                draws += 1;
            }
            triangles += stress.count as i32 / 3 * visible.len() as i32;
        }
        d.cmd_end_render_pass(cmd);
        d.end_command_buffer(cmd)?;
        let prepared = prepare.elapsed().as_secs_f64();
        let submit = Instant::now();
        let wait = [r.acquired[slot]];
        let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let commands = [cmd];
        let signal = [r.finished[idx as usize]];
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .wait_semaphores(&wait)
                .wait_dst_stage_mask(&stages)
                .command_buffers(&commands)
                .signal_semaphores(&signal)],
            r.fence[slot],
        )?;
        swap_api.queue_present(
            queue,
            &vk::PresentInfoKHR::default()
                .wait_semaphores(&signal)
                .swapchains(&[r.swap])
                .image_indices(&[idx]),
        )?;
        rendered += 1;
        frames += 1;
        for (i, value) in [
            waited,
            acquired,
            prepared,
            submit.elapsed().as_secs_f64(),
            culled,
        ]
        .iter()
        .enumerate()
        {
            totals[i] += value;
        }
        if window.elapsed().as_secs_f64() >= 5. {
            log(&format!("VULKAN SCENE {:.2}fps draws={} triangles={} visible={} wait={:.2}ms acquire={:.2}ms cpu-record={:.2}ms submit-present={:.2}ms cull-upload={:.2}ms",frames as f64/window.elapsed().as_secs_f64(),draws,triangles,visible.len(),totals[0]*1000./frames as f64,totals[1]*1000./frames as f64,totals[2]*1000./frames as f64,totals[3]*1000./frames as f64,totals[4]*1000./frames as f64));
            window = Instant::now();
            frames = 0;
            totals = [0.; 5];
        }
    }
    d.device_wait_idle()?;
    log(&format!(
        "VULKAN PASS rendered={rendered} extent={}x{} elapsed={:.3}s",
        extent.width,
        extent.height,
        started.elapsed().as_secs_f64()
    ));
    Ok(())
}
#[no_mangle]
pub unsafe extern "system" fn Java_dev_tcl_rustengine_VulkanActivity_runVulkan(
    env: *mut c_void,
    _class: *mut c_void,
    surface: *mut c_void,
    objects: i32,
    limit: i32,
    tour: u8,
    fixed_time: f32,
    water_mode: u8,
    water_height: i32,
) {
    let window = ANativeWindow_fromSurface(env, surface);
    if window.is_null() {
        log("VULKAN FAIL null native window");
        return;
    }
    if let Err(e) = run(
        window,
        objects.clamp(0, 32768) as usize,
        limit.max(0) as u32,
        tour != 0,
        fixed_time,
        water_mode != 0,
        water_height.clamp(360, 2160) as u32,
    ) {
        log(&format!("VULKAN FAIL {e}"));
    }
    ANativeWindow_release(window);
}
#[no_mangle]
pub extern "system" fn Java_dev_tcl_rustengine_VulkanActivity_stopVulkan(
    _env: *mut c_void,
    _class: *mut c_void,
) {
    STOP.store(true, Ordering::Relaxed);
}

#[no_mangle]
pub extern "system" fn Java_dev_tcl_rustengine_VulkanActivity_prepareVulkan(
    _: *mut c_void,
    _: *mut c_void,
) {
    STOP.store(false, Ordering::Relaxed);
    KEYS.store(0, Ordering::Relaxed);
    TURN.store(0, Ordering::Relaxed);
    MOVE.store(0, Ordering::Relaxed);
}
#[no_mangle]
pub extern "system" fn Java_dev_tcl_rustengine_VulkanActivity_inputVulkan(
    _: *mut c_void,
    _: *mut c_void,
    keys: i32,
    turn: f32,
    movement: f32,
) {
    KEYS.store(keys, Ordering::Relaxed);
    TURN.store(turn.to_bits(), Ordering::Relaxed);
    MOVE.store(movement.to_bits(), Ordering::Relaxed);
}
