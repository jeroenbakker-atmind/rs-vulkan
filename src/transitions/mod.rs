use std::sync::Arc;

use vulkano::command_buffer::{AutoCommandBufferBuilder, PrimaryCommandBufferAbstract};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::layout::{DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo, DescriptorType};
use vulkano::descriptor_set::{DescriptorImageViewInfo, DescriptorSet, WriteDescriptorSet};
use vulkano::device::Device;
use vulkano::image::sampler::Sampler;
use vulkano::image::view::ImageView;
use vulkano::image::ImageLayout;
use vulkano::pipeline::compute::{ComputePipeline, ComputePipelineCreateInfo};
use vulkano::pipeline::layout::{PipelineLayout, PipelineLayoutCreateInfo, PushConstantRange};
use vulkano::pipeline::PipelineShaderStageCreateInfo;
use vulkano::pipeline::PipelineBindPoint;
use vulkano::shader::ShaderStages;

use crate::app::PushConstants;

pub mod smooth;
pub mod fluid;

pub(crate) mod cs_blend_slide {
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

layout(set = 0, binding = 0, rgba16f) uniform image2D u_feedback;
layout(set = 0, binding = 1) uniform sampler2DArray u_slides;

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(u_feedback);
    if (c.x >= sz.x || c.y >= sz.y) return;

    vec2 uv = (vec2(c) + 0.5) / vec2(sz);
    vec4 slide = texture(u_slides, vec3(uv, pc.previous_layer));
    vec4 fb = imageLoad(u_feedback, c);
    vec4 result = mix(fb, vec4(slide.rgb, 1.0), slide.a);
    imageStore(u_feedback, c, result);
}
",
    }
}

pub struct BlendResources {
    pub pipeline: Arc<ComputePipeline>,
    pub pipeline_layout: Arc<PipelineLayout>,
    pub descriptor_set: Arc<DescriptorSet>,
}

pub fn create_blend_resources(
    device: Arc<Device>,
    feedback_view: Arc<ImageView>,
    texture_view: Arc<ImageView>,
    sampler: Arc<Sampler>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
) -> Result<BlendResources, Box<dyn std::error::Error>> {
    let cs_blend_slide_module = cs_blend_slide::load(device.clone())
        .map_err(|e| format!("cs_blend_slide load: {e}"))?;
    let cs_blend_slide_entry = cs_blend_slide_module
        .entry_point("main")
        .ok_or("cs_blend_slide entry point 'main' not found")?;

    let blend_descriptor_set_layout = DescriptorSetLayout::new(
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
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::CombinedImageSampler)
                }),
            ]),
            ..Default::default()
        },
    )?;

    let pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![blend_descriptor_set_layout.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: ShaderStages::COMPUTE,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        blend_descriptor_set_layout.clone(),
        [
            WriteDescriptorSet::image_view_with_layout(
                0,
                DescriptorImageViewInfo {
                    image_view: feedback_view,
                    image_layout: ImageLayout::General,
                },
            ),
            WriteDescriptorSet::image_view_sampler(
                1,
                texture_view,
                sampler,
            ),
        ],
        [],
    )?;

    let pipeline = ComputePipeline::new(
        device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_blend_slide_entry),
            pipeline_layout.clone(),
        ),
    )?;

    Ok(BlendResources { pipeline, pipeline_layout, descriptor_set })
}

pub(crate) fn record_blend_pass<L: PrimaryCommandBufferAbstract>(
    builder: &mut AutoCommandBufferBuilder<L>,
    resources: &BlendResources,
    pc: PushConstants,
    extent: [u32; 2],
) -> Result<(), Box<dyn std::error::Error>> {
    builder
        .bind_pipeline_compute(resources.pipeline.clone())?
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            resources.pipeline_layout.clone(),
            0,
            resources.descriptor_set.clone(),
        )?
        .push_constants(resources.pipeline_layout.clone(), 0, pc)?;
    unsafe { builder.dispatch([(extent[0] + 15) / 16, (extent[1] + 15) / 16, 1])?; }
    Ok(())
}
