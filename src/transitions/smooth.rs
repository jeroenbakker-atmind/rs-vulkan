use std::sync::Arc;

use vulkano::command_buffer::{AutoCommandBufferBuilder, PrimaryCommandBufferAbstract};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::layout::{DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo, DescriptorType};
use vulkano::descriptor_set::{DescriptorImageViewInfo, DescriptorSet, WriteDescriptorSet};
use vulkano::device::Device;
use vulkano::image::view::ImageView;
use vulkano::image::ImageLayout;
use vulkano::pipeline::compute::{ComputePipeline, ComputePipelineCreateInfo};
use vulkano::pipeline::layout::{PipelineLayout, PipelineLayoutCreateInfo, PushConstantRange};
use vulkano::pipeline::PipelineShaderStageCreateInfo;
use vulkano::pipeline::PipelineBindPoint;
use vulkano::shader::ShaderStages;

use crate::app::PushConstants;

pub(crate) mod cs_blur_h {
    vulkano_shaders::shader! {
        ty: "compute",
        src: r"
#version 460
layout(local_size_x = 16, local_size_y = 16) in;

layout(push_constant) uniform PC {
    int   current_layer;
    int   previous_layer;
    float blur_radius;
    float slide_offset_x;
    float slide_offset_y;
} pc;

layout(set = 0, binding = 0, rgba16f) uniform readonly image2D src;
layout(set = 0, binding = 1, rgba16f) uniform writeonly image2D dst;

shared vec4 cache[16][16];

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(src);
    if (c.x >= sz.x || c.y >= sz.y) return;

    int r = int(max(1.0, pc.blur_radius));
    float sigma = float(r) * 0.333;
    vec4 accum = vec4(0.0);
    float total = 0.0;

    for (int x = -r; x <= r; x++) {
        ivec2 p = ivec2(clamp(c.x + x, 0, sz.x - 1), c.y);
        float w = exp(-float(x * x) / (2.0 * sigma * sigma));
        accum += imageLoad(src, p) * w;
        total += w;
    }

    imageStore(dst, c, accum / total);
}
",
    }
}

pub(crate) mod cs_blur_v {
    vulkano_shaders::shader! {
        ty: "compute",
        src: r"
#version 460
layout(local_size_x = 16, local_size_y = 16) in;

layout(push_constant) uniform PC {
    int   current_layer;
    int   previous_layer;
    float blur_radius;
    float slide_offset_x;
    float slide_offset_y;
} pc;

layout(set = 0, binding = 0, rgba16f) uniform readonly image2D src;
layout(set = 0, binding = 1, rgba16f) uniform writeonly image2D dst;

shared vec4 cache[16][16];

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(src);
    if (c.x >= sz.x || c.y >= sz.y) return;

    int r = int(max(1.0, pc.blur_radius));
    float sigma = float(r) * 0.333;
    vec4 accum = vec4(0.0);
    float total = 0.0;

    for (int y = -r; y <= r; y++) {
        ivec2 p = ivec2(c.x, clamp(c.y + y, 0, sz.y - 1));
        float w = exp(-float(y * y) / (2.0 * sigma * sigma));
        accum += imageLoad(src, p) * w;
        total += w;
    }

    imageStore(dst, c, accum / total);
}
",
    }
}

pub struct SmoothResources {
    pub blur_h_pipeline: Arc<ComputePipeline>,
    pub blur_v_pipeline: Arc<ComputePipeline>,
    pub blur_pipeline_layout: Arc<PipelineLayout>,
    pub blur_h_descriptor_set: Arc<DescriptorSet>,
    pub blur_v_descriptor_set: Arc<DescriptorSet>,
}

pub fn create_smooth_resources(
    device: Arc<Device>,
    feedback_view: Arc<ImageView>,
    ping_view: Arc<ImageView>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
) -> Result<SmoothResources, Box<dyn std::error::Error>> {
    let cs_blur_module = cs_blur_h::load(device.clone())
        .map_err(|e| format!("cs_blur_h load: {e}"))?;
    let cs_blur_v_module = cs_blur_v::load(device.clone())
        .map_err(|e| format!("cs_blur_v load: {e}"))?;

    let cs_blur_entry = cs_blur_module
        .entry_point("main")
        .ok_or("cs_blur_h entry point 'main' not found")?;
    let cs_blur_v_entry = cs_blur_v_module
        .entry_point("main")
        .ok_or("cs_blur_v entry point 'main' not found")?;

    let blur_descriptor_set_layout = DescriptorSetLayout::new(
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

    let blur_pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![blur_descriptor_set_layout.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: ShaderStages::COMPUTE,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    let make_blur_set = |src: Arc<ImageView>, dst: Arc<ImageView>| -> Result<Arc<DescriptorSet>, Box<dyn std::error::Error>> {
        DescriptorSet::new(
            descriptor_set_allocator.clone(),
            blur_descriptor_set_layout.clone(),
            [
                WriteDescriptorSet::image_view_with_layout(
                    0,
                    DescriptorImageViewInfo { image_view: src, image_layout: ImageLayout::General },
                ),
                WriteDescriptorSet::image_view_with_layout(
                    1,
                    DescriptorImageViewInfo { image_view: dst, image_layout: ImageLayout::General },
                ),
            ],
            [],
        )
        .map_err(|e| format!("blur descriptor_set: {e}").into())
    };

    let blur_h_set = make_blur_set(feedback_view.clone(), ping_view.clone())?;
    let blur_v_set = make_blur_set(ping_view.clone(), feedback_view.clone())?;

    let blur_h_pipeline = ComputePipeline::new(
        device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_blur_entry),
            blur_pipeline_layout.clone(),
        ),
    )?;

    let blur_v_pipeline = ComputePipeline::new(
        device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_blur_v_entry),
            blur_pipeline_layout.clone(),
        ),
    )?;

    Ok(SmoothResources {
        blur_h_pipeline,
        blur_v_pipeline,
        blur_pipeline_layout,
        blur_h_descriptor_set: blur_h_set,
        blur_v_descriptor_set: blur_v_set,
    })
}

pub(crate) fn record_blur_passes<L: PrimaryCommandBufferAbstract>(
    builder: &mut AutoCommandBufferBuilder<L>,
    resources: &SmoothResources,
    pc: PushConstants,
    extent: [u32; 2],
) -> Result<(), Box<dyn std::error::Error>> {
    let ext_x = (extent[0] + 15) / 16;
    let ext_y = (extent[1] + 15) / 16;

    builder
        .bind_pipeline_compute(resources.blur_h_pipeline.clone())?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            resources.blur_pipeline_layout.clone(),
            0,
            resources.blur_h_descriptor_set.clone(),
        )?
        .push_constants(resources.blur_pipeline_layout.clone(), 0, pc)?;
    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

    builder
        .bind_pipeline_compute(resources.blur_v_pipeline.clone())?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            resources.blur_pipeline_layout.clone(),
            0,
            resources.blur_v_descriptor_set.clone(),
        )?
        .push_constants(resources.blur_pipeline_layout.clone(), 0, pc)?;
    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

    Ok(())
}
