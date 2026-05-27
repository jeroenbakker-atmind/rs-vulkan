use std::sync::Arc;

use vulkano::command_buffer::{AutoCommandBufferBuilder, PrimaryCommandBufferAbstract};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::layout::{DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo, DescriptorType};
use vulkano::descriptor_set::{DescriptorImageViewInfo, DescriptorSet, WriteDescriptorSet};
use vulkano::device::Device;
use vulkano::image::view::ImageView;
use vulkano::image::ImageLayout;
use vulkano::memory::allocator::StandardMemoryAllocator;
use vulkano::pipeline::compute::{ComputePipeline, ComputePipelineCreateInfo};
use vulkano::pipeline::layout::{PipelineLayout, PipelineLayoutCreateInfo, PushConstantRange};
use vulkano::pipeline::PipelineShaderStageCreateInfo;
use vulkano::pipeline::PipelineBindPoint;
use vulkano::shader::ShaderStages;

use crate::app::PushConstants;

pub(crate) mod cs_fluid_advect {
    vulkano_shaders::shader! {
        ty: "compute",
        src: r"
#version 460
layout(local_size_x = 16, local_size_y = 16) in;

layout(push_constant) uniform PC {
    int   unused0;
    int   unused1;
    float dt;
    float dx;
    float dy;
} pc;

layout(set = 0, binding = 0, rgba16f) uniform readonly image2D u_src;
layout(set = 0, binding = 1, rgba16f) uniform readonly image2D u_vel;
layout(set = 0, binding = 2, rgba16f) uniform writeonly image2D u_dst;

float catrom_weight(float d) {
    d = abs(d);
    if (d < 1.0) {
        return 1.5 * d * d * d - 2.5 * d * d + 1.0;
    } else if (d < 2.0) {
        return -0.5 * d * d * d + 2.5 * d * d - 4.0 * d + 2.0;
    } else {
        return 0.0;
    }
}

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(u_src);
    if (c.x >= sz.x || c.y >= sz.y) return;

    vec2 uv = (vec2(c) + 0.5) / vec2(sz);
    vec2 vel = imageLoad(u_vel, c).rg;
    vec2 src_uv = uv - vel * pc.dt;
    src_uv = clamp(src_uv, vec2(0.0), vec2(1.0));

    // Catmull-Rom bicubic interpolation (preserves high frequencies
    // much better than bilinear, reducing numerical diffusion)
    vec2 f = src_uv * vec2(sz) - 0.5;
    ivec2 ic = ivec2(floor(f));
    vec2 fr = f - vec2(ic);

    vec4 result = vec4(0.0);
    for (int dy = -1; dy <= 2; dy++) {
        for (int dx = -1; dx <= 2; dx++) {
            ivec2 p = clamp(ic + ivec2(dx, dy), ivec2(0), sz - 1);
            float ddx = float(dx) - fr.x;
            float ddy = float(dy) - fr.y;
            float wx = catrom_weight(ddx);
            float wy = catrom_weight(ddy);
            result += imageLoad(u_src, p) * wx * wy;
        }
    }

    imageStore(u_dst, c, result);
}
",
    }
}

pub(crate) mod cs_fluid_divergence {
    vulkano_shaders::shader! {
        ty: "compute",
        src: r"
#version 460
layout(local_size_x = 16, local_size_y = 16) in;

layout(push_constant) uniform PC {
    int   unused0;
    int   unused1;
    float unused2;
    float dx;
    float dy;
} pc;

layout(set = 0, binding = 0, rgba16f) uniform readonly image2D u_vel;
layout(set = 0, binding = 1, rgba16f) uniform writeonly image2D u_div;

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(u_vel);
    if (c.x >= sz.x || c.y >= sz.y) return;

    ivec2 r = ivec2(min(c.x + 1, sz.x - 1), c.y);
    ivec2 l = ivec2(max(c.x - 1, 0), c.y);
    ivec2 d = ivec2(c.x, min(c.y + 1, sz.y - 1));
    ivec2 u = ivec2(c.x, max(c.y - 1, 0));

    float vx_r = imageLoad(u_vel, r).r;
    float vx_l = imageLoad(u_vel, l).r;
    float vy_d = imageLoad(u_vel, d).g;
    float vy_u = imageLoad(u_vel, u).g;

    float div = -0.5 * (vx_r - vx_l + vy_d - vy_u);
    imageStore(u_div, c, vec4(div, 0.0, 0.0, 0.0));
}
",
    }
}

pub(crate) mod cs_fluid_jacobi {
    vulkano_shaders::shader! {
        ty: "compute",
        src: r"
#version 460
layout(local_size_x = 16, local_size_y = 16) in;

layout(push_constant) uniform PC {
    int   unused0;
    int   unused1;
    float alpha;
    float dx;
    float dy;
} pc;

layout(set = 0, binding = 0, rgba16f) uniform readonly image2D u_div;
layout(set = 0, binding = 1, rgba16f) uniform readonly image2D u_p;
layout(set = 0, binding = 2, rgba16f) uniform writeonly image2D u_p_next;

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(u_div);
    if (c.x >= sz.x || c.y >= sz.y) return;

    ivec2 r = ivec2(min(c.x + 1, sz.x - 1), c.y);
    ivec2 l = ivec2(max(c.x - 1, 0), c.y);
    ivec2 d = ivec2(c.x, min(c.y + 1, sz.y - 1));
    ivec2 u = ivec2(c.x, max(c.y - 1, 0));

    float p_l = imageLoad(u_p, l).r;
    float p_r = imageLoad(u_p, r).r;
    float p_u = imageLoad(u_p, u).r;
    float p_d = imageLoad(u_p, d).r;
    float div = imageLoad(u_div, c).r;

    float p_new = (p_l + p_r + p_u + p_d + pc.alpha * div) / 4.0;
    imageStore(u_p_next, c, vec4(p_new, 0.0, 0.0, 0.0));
}
",
    }
}

pub(crate) mod cs_fluid_gradient_subtract {
    vulkano_shaders::shader! {
        ty: "compute",
        src: r"
#version 460
layout(local_size_x = 16, local_size_y = 16) in;

layout(push_constant) uniform PC {
    int   unused0;
    int   unused1;
    float unused2;
    float dx;
    float dy;
} pc;

layout(set = 0, binding = 0, rgba16f) uniform readonly image2D u_p;
layout(set = 0, binding = 1, rgba16f) uniform readonly image2D u_vel_in;
layout(set = 0, binding = 2, rgba16f) uniform writeonly image2D u_vel_out;

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(u_vel_in);
    if (c.x >= sz.x || c.y >= sz.y) return;

    ivec2 r = ivec2(min(c.x + 1, sz.x - 1), c.y);
    ivec2 l = ivec2(max(c.x - 1, 0), c.y);
    ivec2 d = ivec2(c.x, min(c.y + 1, sz.y - 1));
    ivec2 up = ivec2(c.x, max(c.y - 1, 0));

    float p_r = imageLoad(u_p, r).r;
    float p_l = imageLoad(u_p, l).r;
    float p_d = imageLoad(u_p, d).r;
    float p_u = imageLoad(u_p, up).r;

    vec2 vel = imageLoad(u_vel_in, c).rg;
    vel.x -= 0.5 * (p_r - p_l);
    vel.y -= 0.5 * (p_d - p_u);
    imageStore(u_vel_out, c, vec4(vel.x, vel.y, 0.0, 0.0));
}
",
    }
}

pub(crate) mod cs_fluid_add_buoyancy {
    vulkano_shaders::shader! {
        ty: "compute",
        src: r"
#version 460
layout(local_size_x = 16, local_size_y = 16) in;

layout(push_constant) uniform PC {
    int   unused0;
    int   unused1;
    float dt;
    float dx;
    float dy;
} pc;

layout(set = 0, binding = 0, rgba16f) uniform readonly image2D u_feedback;
layout(set = 0, binding = 1, rgba16f) uniform image2D u_vel;

float luminance(vec3 c) {
    return dot(c, vec3(0.299, 0.587, 0.114));
}

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(u_vel);
    if (c.x >= sz.x || c.y >= sz.y) return;

    ivec2 r = ivec2(min(c.x + 1, sz.x - 1), c.y);
    ivec2 l = ivec2(max(c.x - 1, 0), c.y);
    ivec2 d = ivec2(c.x, min(c.y + 1, sz.y - 1));
    ivec2 up = ivec2(c.x, max(c.y - 1, 0));

    float lum_r = luminance(imageLoad(u_feedback, r).rgb);
    float lum_l = luminance(imageLoad(u_feedback, l).rgb);
    float lum_d = luminance(imageLoad(u_feedback, d).rgb);
    float lum_u = luminance(imageLoad(u_feedback, up).rgb);

    vec2 grad = vec2(lum_r - lum_l, lum_d - lum_u) * pc.dt;
    vec2 existing_vel = imageLoad(u_vel, c).rg;
    imageStore(u_vel, c, vec4(existing_vel * pc.dx + grad, 0.0, 0.0));
}
",
    }
}

#[allow(dead_code)]
pub struct FluidResources {
    pub velocity: Arc<vulkano::image::Image>,
    pub velocity_view: Arc<ImageView>,
    pub velocity_ping: Arc<vulkano::image::Image>,
    pub velocity_ping_view: Arc<ImageView>,
    pub pressure: Arc<vulkano::image::Image>,
    pub pressure_view: Arc<ImageView>,
    pub pressure_ping: Arc<vulkano::image::Image>,
    pub pressure_ping_view: Arc<ImageView>,
    pub divergence: Arc<vulkano::image::Image>,
    pub divergence_view: Arc<ImageView>,
    pub ds_2binding_layout: Arc<vulkano::descriptor_set::layout::DescriptorSetLayout>,
    pub ds_3binding_layout: Arc<vulkano::descriptor_set::layout::DescriptorSetLayout>,
    pub pipeline_2binding_layout: Arc<PipelineLayout>,
    pub pipeline_3binding_layout: Arc<PipelineLayout>,
    pub advect_pipeline: Arc<ComputePipeline>,
    pub divergence_pipeline: Arc<ComputePipeline>,
    pub jacobi_pipeline: Arc<ComputePipeline>,
    pub gradient_subtract_pipeline: Arc<ComputePipeline>,
    pub buoyancy_pipeline: Arc<ComputePipeline>,
    pub advect_velocity_ds: Arc<DescriptorSet>,
    pub advect_density_h_ds: Arc<DescriptorSet>,
    pub advect_density_v_ds: Arc<DescriptorSet>,
    pub divergence_ds: Arc<DescriptorSet>,
    pub jacobi_h_ds: Arc<DescriptorSet>,
    pub jacobi_v_ds: Arc<DescriptorSet>,
    pub gradient_subtract_ds: Arc<DescriptorSet>,
    pub buoyancy_ds: Arc<DescriptorSet>,
}

pub fn create_fluid_resources(
    device: Arc<Device>,
    allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    feedback_view: Arc<ImageView>,
    ping_view: Arc<ImageView>,
    extent: [u32; 2],
) -> Result<FluidResources, Box<dyn std::error::Error>> {
    use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage};
    use vulkano::memory::allocator::AllocationCreateInfo;

    let make_fluid_image = |extent: [u32; 2]| -> Result<(Arc<Image>, Arc<ImageView>), Box<dyn std::error::Error>> {
        let img = Image::new(
            allocator.clone() as Arc<dyn vulkano::memory::allocator::MemoryAllocator>,
            ImageCreateInfo {
                image_type: ImageType::Dim2d,
                format: vulkano::format::Format::R16G16B16A16_SFLOAT,
                extent: [extent[0], extent[1], 1],
                array_layers: 1,
                mip_levels: 1,
                usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
                ..Default::default()
            },
            AllocationCreateInfo::default(),
        )?;
        let view = ImageView::new(
            img.clone(),
            vulkano::image::view::ImageViewCreateInfo {
                view_type: vulkano::image::view::ImageViewType::Dim2d,
                format: vulkano::format::Format::R16G16B16A16_SFLOAT,
                subresource_range: vulkano::image::ImageSubresourceRange {
                    aspects: vulkano::image::ImageAspects::COLOR,
                    mip_levels: 0..1,
                    array_layers: 0..1,
                },
                ..Default::default()
            },
        )?;
        Ok((img, view))
    };

    let (velocity, velocity_view) = make_fluid_image(extent)?;
    let (velocity_ping, velocity_ping_view) = make_fluid_image(extent)?;
    let (pressure, pressure_view) = make_fluid_image(extent)?;
    let (pressure_ping, pressure_ping_view) = make_fluid_image(extent)?;
    let (divergence, divergence_view) = make_fluid_image(extent)?;

    // Load shader modules
    let cs_advect_module = cs_fluid_advect::load(device.clone())
        .map_err(|e| format!("cs_fluid_advect load: {e}"))?;
    let cs_divergence_module = cs_fluid_divergence::load(device.clone())
        .map_err(|e| format!("cs_fluid_divergence load: {e}"))?;
    let cs_jacobi_module = cs_fluid_jacobi::load(device.clone())
        .map_err(|e| format!("cs_fluid_jacobi load: {e}"))?;
    let cs_gradient_subtract_module = cs_fluid_gradient_subtract::load(device.clone())
        .map_err(|e| format!("cs_fluid_gradient_subtract load: {e}"))?;
    let cs_buoyancy_module = cs_fluid_add_buoyancy::load(device.clone())
        .map_err(|e| format!("cs_fluid_add_buoyancy load: {e}"))?;

    let cs_advect_entry = cs_advect_module
        .entry_point("main")
        .ok_or("cs_fluid_advect entry point 'main' not found")?;
    let cs_divergence_entry = cs_divergence_module
        .entry_point("main")
        .ok_or("cs_fluid_divergence entry point 'main' not found")?;
    let cs_jacobi_entry = cs_jacobi_module
        .entry_point("main")
        .ok_or("cs_fluid_jacobi entry point 'main' not found")?;
    let cs_gradient_subtract_entry = cs_gradient_subtract_module
        .entry_point("main")
        .ok_or("cs_fluid_gradient_subtract entry point 'main' not found")?;
    let cs_buoyancy_entry = cs_buoyancy_module
        .entry_point("main")
        .ok_or("cs_fluid_add_buoyancy entry point 'main' not found")?;

    // 2-binding layout
    let ds_2binding = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([
                (0u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
                (1u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
            ]),
            ..Default::default()
        },
    )?;

    let pipeline_2binding = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![ds_2binding.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: ShaderStages::COMPUTE,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    // 3-binding layout
    let ds_3binding = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([
                (0u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
                (1u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
                (2u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
            ]),
            ..Default::default()
        },
    )?;

    let pipeline_3binding = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![ds_3binding.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: ShaderStages::COMPUTE,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    // Compute pipelines
    let advect_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_advect_entry),
            pipeline_3binding.clone(),
        ),
    )?;

    let divergence_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_divergence_entry),
            pipeline_2binding.clone(),
        ),
    )?;

    let jacobi_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_jacobi_entry),
            pipeline_3binding.clone(),
        ),
    )?;

    let gradient_subtract_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_gradient_subtract_entry),
            pipeline_3binding.clone(),
        ),
    )?;

    let buoyancy_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_buoyancy_entry),
            pipeline_2binding.clone(),
        ),
    )?;

    // Helper to create 2-binding descriptor set
    let make_2binding_set = |b0: Arc<ImageView>, b1: Arc<ImageView>| -> Result<Arc<DescriptorSet>, Box<dyn std::error::Error>> {
        DescriptorSet::new(
            descriptor_set_allocator.clone(),
            ds_2binding.clone(),
            [
                WriteDescriptorSet::image_view_with_layout(
                    0, DescriptorImageViewInfo { image_view: b0, image_layout: ImageLayout::General },
                ),
                WriteDescriptorSet::image_view_with_layout(
                    1, DescriptorImageViewInfo { image_view: b1, image_layout: ImageLayout::General },
                ),
            ],
            [],
        )
        .map_err(|e| format!("fluid 2binding ds: {e}").into())
    };

    // Helper to create 3-binding descriptor set
    let make_3binding_set = |b0: Arc<ImageView>, b1: Arc<ImageView>, b2: Arc<ImageView>| -> Result<Arc<DescriptorSet>, Box<dyn std::error::Error>> {
        DescriptorSet::new(
            descriptor_set_allocator.clone(),
            ds_3binding.clone(),
            [
                WriteDescriptorSet::image_view_with_layout(
                    0, DescriptorImageViewInfo { image_view: b0, image_layout: ImageLayout::General },
                ),
                WriteDescriptorSet::image_view_with_layout(
                    1, DescriptorImageViewInfo { image_view: b1, image_layout: ImageLayout::General },
                ),
                WriteDescriptorSet::image_view_with_layout(
                    2, DescriptorImageViewInfo { image_view: b2, image_layout: ImageLayout::General },
                ),
            ],
            [],
        )
        .map_err(|e| format!("fluid 3binding ds: {e}").into())
    };

    let advect_velocity_ds = make_3binding_set(
        velocity_view.clone(), velocity_view.clone(), velocity_ping_view.clone(),
    )?;
    let advect_density_h_ds = make_3binding_set(
        feedback_view.clone(), velocity_view.clone(), ping_view.clone(),
    )?;
    let advect_density_v_ds = make_3binding_set(
        ping_view.clone(), velocity_view.clone(), feedback_view.clone(),
    )?;
    let divergence_ds = make_2binding_set(
        velocity_ping_view.clone(), divergence_view.clone(),
    )?;
    let jacobi_h_ds = make_3binding_set(
        divergence_view.clone(), pressure_view.clone(), pressure_ping_view.clone(),
    )?;
    let jacobi_v_ds = make_3binding_set(
        divergence_view.clone(), pressure_ping_view.clone(), pressure_view.clone(),
    )?;
    let gradient_subtract_ds = make_3binding_set(
        pressure_view.clone(), velocity_ping_view.clone(), velocity_view.clone(),
    )?;
    let buoyancy_ds = make_2binding_set(
        feedback_view.clone(), velocity_view.clone(),
    )?;

    Ok(FluidResources {
        velocity, velocity_view,
        velocity_ping, velocity_ping_view,
        pressure, pressure_view,
        pressure_ping, pressure_ping_view,
        divergence, divergence_view,
        ds_2binding_layout: ds_2binding,
        ds_3binding_layout: ds_3binding,
        pipeline_2binding_layout: pipeline_2binding,
        pipeline_3binding_layout: pipeline_3binding,
        advect_pipeline,
        divergence_pipeline,
        jacobi_pipeline,
        gradient_subtract_pipeline,
        buoyancy_pipeline,
        advect_velocity_ds,
        advect_density_h_ds,
        advect_density_v_ds,
        divergence_ds,
        jacobi_h_ds,
        jacobi_v_ds,
        gradient_subtract_ds,
        buoyancy_ds,
    })
}

/// Records all fluid simulation compute dispatches for one frame.
/// `transition_blended` tracks whether the first-frame blend has been done.
pub(crate) fn record_fluid_simulation<L: PrimaryCommandBufferAbstract>(
    builder: &mut AutoCommandBufferBuilder<L>,
    resources: &FluidResources,
    _pc: PushConstants,
    dt: f32,
    extent: [u32; 2],
) -> Result<(), Box<dyn std::error::Error>> {
    let ext_x = (extent[0] + 15) / 16;
    let ext_y = (extent[1] + 15) / 16;

    // Add buoyancy force from density gradient to velocity (every frame).
    {
        let damping = (-dt * 3.0).exp();
        let buoy_pc = PushConstants {
            current_layer: 0, previous_layer: 0,
            blur_radius: dt * 50.0,
            slide_offset_x: damping, slide_offset_y: 1.0,
        };
        builder
            .bind_pipeline_compute(resources.buoyancy_pipeline.clone())?
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                resources.pipeline_2binding_layout.clone(), 0,
                resources.buoyancy_ds.clone(),
            )?
            .push_constants(resources.pipeline_2binding_layout.clone(), 0, buoy_pc)?;
        unsafe { builder.dispatch([ext_x, ext_y, 1])?; }
    }

    // PC template for advection steps
    let advect_pc = PushConstants {
        current_layer: 0, previous_layer: 0,
        blur_radius: 0.5,
        slide_offset_x: 1.0, slide_offset_y: 1.0,
    };

    // PC for divergence / gradient subtract (dx, dy = 1.0 in pixel space)
    let identity_pc = PushConstants {
        current_layer: 0, previous_layer: 0,
        blur_radius: 0.0,
        slide_offset_x: 1.0, slide_offset_y: 1.0,
    };

    // PC for Jacobi
    let jacobi_pc = PushConstants {
        current_layer: 0, previous_layer: 0,
        blur_radius: 1.0,
        slide_offset_x: 1.0, slide_offset_y: 1.0,
    };

    // 1. Self-advect velocity field: velocity → velocity_ping
    builder
        .bind_pipeline_compute(resources.advect_pipeline.clone())?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            resources.pipeline_3binding_layout.clone(), 0,
            resources.advect_velocity_ds.clone(),
        )?
        .push_constants(resources.pipeline_3binding_layout.clone(), 0, advect_pc)?;
    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

    // 2. Compute divergence of velocity_ping → divergence
    builder
        .bind_pipeline_compute(resources.divergence_pipeline.clone())?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            resources.pipeline_2binding_layout.clone(), 0,
            resources.divergence_ds.clone(),
        )?
        .push_constants(resources.pipeline_2binding_layout.clone(), 0, identity_pc)?;
    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

    // 3. Jacobi iterations: solve pressure Poisson (10 iters)
    for i in 0..10 {
        let ds = if i % 2 == 0 {
            &resources.jacobi_h_ds
        } else {
            &resources.jacobi_v_ds
        };
        builder
            .bind_pipeline_compute(resources.jacobi_pipeline.clone())?
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                resources.pipeline_3binding_layout.clone(), 0,
                ds.clone(),
            )?
            .push_constants(resources.pipeline_3binding_layout.clone(), 0, jacobi_pc)?;
        unsafe { builder.dispatch([ext_x, ext_y, 1])?; }
    }

    // 4. Subtract pressure gradient: project velocity_ping → velocity
    builder
        .bind_pipeline_compute(resources.gradient_subtract_pipeline.clone())?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            resources.pipeline_3binding_layout.clone(), 0,
            resources.gradient_subtract_ds.clone(),
        )?
        .push_constants(resources.pipeline_3binding_layout.clone(), 0, identity_pc)?;
    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

    // 5a. Advect density: feedback → ping
    builder
        .bind_pipeline_compute(resources.advect_pipeline.clone())?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            resources.pipeline_3binding_layout.clone(), 0,
            resources.advect_density_h_ds.clone(),
        )?
        .push_constants(resources.pipeline_3binding_layout.clone(), 0, advect_pc)?;
    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

    // 5b. Advect density: ping → feedback
    builder
        .bind_pipeline_compute(resources.advect_pipeline.clone())?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            resources.pipeline_3binding_layout.clone(), 0,
            resources.advect_density_v_ds.clone(),
        )?
        .push_constants(resources.pipeline_3binding_layout.clone(), 0, advect_pc)?;
    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

    Ok(())
}
