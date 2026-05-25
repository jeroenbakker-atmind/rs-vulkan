use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use half::f16;
use smallvec::smallvec;
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, ClearColorImageInfo, PrimaryCommandBufferAbstract,
    RenderingAttachmentInfo, RenderingInfo,
};
use vulkano::command_buffer::CopyBufferToImageInfo;
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::layout::{
    DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo, DescriptorType,
};
use vulkano::descriptor_set::{DescriptorImageViewInfo, DescriptorSet, WriteDescriptorSet};
use vulkano::device::physical::PhysicalDeviceType;
use vulkano::device::{
    Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, QueueCreateInfo, QueueFlags,
};
use vulkano::image::sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo, SamplerMipmapMode};
use vulkano::image::view::{ImageView, ImageViewCreateInfo, ImageViewType};
use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage, ImageSubresourceRange, ImageAspects, ImageLayout};
use vulkano::instance::{Instance, InstanceCreateInfo, InstanceExtensions};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::{CullMode, RasterizationState};
use vulkano::pipeline::graphics::vertex_input::VertexInputState;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::graphics::viewport::Scissor;
use vulkano::pipeline::graphics::subpass::{PipelineRenderingCreateInfo, PipelineSubpassType};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::layout::{PipelineLayout, PipelineLayoutCreateInfo, PushConstantRange};
use vulkano::pipeline::DynamicState;
use vulkano::pipeline::PipelineBindPoint;
use vulkano::pipeline::PipelineShaderStageCreateInfo;
use vulkano::pipeline::compute::{ComputePipeline, ComputePipelineCreateInfo};
use vulkano::render_pass::{AttachmentLoadOp, AttachmentStoreOp};
use vulkano::format::{ClearValue, Format};
use vulkano::swapchain::{
    PresentMode, Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo, SurfaceInfo,
};
use vulkano::sync::GpuFuture;
use vulkano::{VulkanLibrary};

use crate::texture;

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: r"
#version 460
layout(location = 0) out vec2 v_uv;
void main() {
    vec2 pos = vec2(
        (gl_VertexIndex == 2) ? 3.0 : -1.0,
        (gl_VertexIndex == 1) ? 3.0 : -1.0
    );
    v_uv = pos * 0.5 + 0.5;
    gl_Position = vec4(pos, 0.0, 1.0);
}
",
    }
}

mod cs_blur {
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

    // direction of blur is the X-axis (set in the shader specialization via swapping)
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

mod cs_blur_v {
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

mod cs_blend_slide {
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

// ---------------------------------------------------------------------------
// Stable Fluids Simulation shaders
// ---------------------------------------------------------------------------

// Semi-Lagrangian advection: reads a field at position x - velocity*dt
// Uses 3 storage images: src (readonly), velocity field (readonly), dst (writeonly)
mod cs_fluid_advect {
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

void main() {
    ivec2 c = ivec2(gl_GlobalInvocationID.xy);
    ivec2 sz = imageSize(u_src);
    if (c.x >= sz.x || c.y >= sz.y) return;

    vec2 uv = (vec2(c) + 0.5) / vec2(sz);
    vec2 vel = imageLoad(u_vel, c).rg;
    vec2 src_uv = uv - vel * pc.dt;
    src_uv = clamp(src_uv, vec2(0.0), vec2(1.0));

    // Manual bilinear interpolation
    vec2 f = src_uv * vec2(sz) - 0.5;
    ivec2 ic = ivec2(floor(f));
    vec2 fr = f - vec2(ic);
    ivec2 c00 = clamp(ic + ivec2(0, 0), ivec2(0), sz - 1);
    ivec2 c10 = clamp(ic + ivec2(1, 0), ivec2(0), sz - 1);
    ivec2 c01 = clamp(ic + ivec2(0, 1), ivec2(0), sz - 1);
    ivec2 c11 = clamp(ic + ivec2(1, 1), ivec2(0), sz - 1);
    vec4 v00 = imageLoad(u_src, c00);
    vec4 v10 = imageLoad(u_src, c10);
    vec4 v01 = imageLoad(u_src, c01);
    vec4 v11 = imageLoad(u_src, c11);
    vec4 result = mix(mix(v00, v10, fr.x), mix(v01, v11, fr.x), fr.y);

    imageStore(u_dst, c, result);
}
",
    }
}

// Compute divergence of velocity field
mod cs_fluid_divergence {
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

    // Central difference divergence with sign convention for Poisson solve
    float div = -0.5 * (vx_r - vx_l + vy_d - vy_u);
    imageStore(u_div, c, vec4(div, 0.0, 0.0, 0.0));
}
",
    }
}

// One Jacobi iteration for the Poisson pressure solve
mod cs_fluid_jacobi {
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

    // Jacobi: p_new = (p_left + p_right + p_up + p_down + alpha * div) / beta
    // For Poisson: alpha = -1, beta = 4, or with our sign convention alpha = 1
    float p_new = (p_l + p_r + p_u + p_d + pc.alpha * div) / 4.0;
    imageStore(u_p_next, c, vec4(p_new, 0.0, 0.0, 0.0));
}
",
    }
}

// Subtract pressure gradient from velocity to make it divergence-free
// Reads velocity_in + pressure, writes corrected velocity_out
mod cs_fluid_gradient_subtract {
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

// Initialize velocity field from feedback image gradient (first frame)
mod cs_fluid_init_velocity {
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
layout(set = 0, binding = 1, rgba16f) uniform writeonly image2D u_vel;

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

    vec2 vel = vec2(lum_r - lum_l, lum_d - lum_u) * pc.dt;
    imageStore(u_vel, c, vec4(vel.x, vel.y, 0.0, 0.0));
}
",
    }
}

mod fs_present {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"
#version 460
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;

layout(push_constant) uniform PC {
    int   current_layer;
    int   previous_layer;
    float blur_radius;
    float slide_offset_x;
    float slide_offset_y;
} pc;

layout(set = 0, binding = 0) uniform sampler2D feedback;
layout(set = 1, binding = 0) uniform sampler2DArray slides;

void main() {
    vec3 fb = texture(feedback, v_uv).rgb;
    vec4 slide = texture(slides, vec3(v_uv, pc.current_layer));
    vec3 result = mix(fb, slide.rgb, slide.a);
    f_color = vec4(result, 1.0);
}
",
    }
}

mod fs_direct {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"
#version 460
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;

layout(push_constant) uniform PC {
    int   current_layer;
    int   previous_layer;
    float blur_radius;
    float slide_offset_x;
    float slide_offset_y;
} pc;

layout(set = 0, binding = 0) uniform sampler2DArray slides;

void main() {
    vec3 slide;

    // Slide transition if offset is active
    float has_slide = float(pc.slide_offset_x != 0.0 || pc.slide_offset_y != 0.0);
    if (has_slide > 0.5) {
        vec3 prev = texture(slides, vec3(v_uv, pc.previous_layer)).rgb;
        vec2 slide_uv = v_uv - vec2(pc.slide_offset_x, pc.slide_offset_y);
        float in_slide = float(
            slide_uv.x >= 0.0 && slide_uv.x <= 1.0 &&
            slide_uv.y >= 0.0 && slide_uv.y <= 1.0
        );
        vec3 slide_curr = texture(slides, vec3(clamp(slide_uv, vec2(0.0), vec2(1.0)), pc.current_layer)).rgb;
        slide = mix(prev, slide_curr, in_slide);
    } else {
        slide = texture(slides, vec3(v_uv, pc.current_layer)).rgb;
    }

    f_color = vec4(slide, 1.0);
}
",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionType {
    Smooth,
    Instant,
    Slide,
    Fluid,
}

#[derive(Clone)]
pub struct AppConfig {
    pub slides_path: std::path::PathBuf,
    pub transition_type: TransitionType,
    pub transition_duration: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            slides_path: std::path::PathBuf::new(),
            transition_type: TransitionType::Smooth,
            transition_duration: 0.5,
        }
    }
}

pub fn print_usage() {
    eprintln!(
        "RS-Vulkan Slides
Usage: rs-vulkan <slides-folder> [options]
       rs-vulkan init <path>

Arguments:
  <slides-folder>              Directory containing chapter_slide.png files

Commands:
  init <path>                  Create an example presentation at <path>

Options:
  --transition-type <type>     Transition style: smooth (default), instant, slide, or fluid
  --transition-duration <sec>  Transition duration in seconds (default: 0.5)
  --help                       Show this help

Transition types:
  smooth  - Compute-shader blur with single feedback buffer (default)
  instant - No animation, immediate cut
  slide   - New slide slides in; from bottom for slides, from right for chapters
  fluid   - Stable fluids advection for each color channel"
    );
}

pub fn parse_args(args: &[String]) -> Option<AppConfig> {
    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage();
        return None;
    }

    if args.get(1).map_or(false, |a| a == "init") {
        let path = match args.get(2) {
            Some(p) => std::path::PathBuf::from(p),
            None => {
                eprintln!("Error: 'init' requires a path argument");
                print_usage();
                return None;
            }
        };
        init_example_presentation(&path);
        println!("Created example presentation at '{}'", path.display());
        return None;
    }

    let mut config = AppConfig::default();
    config.slides_path = std::path::PathBuf::from(&args[1]);

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--transition-type" => {
                i += 1;
                config.transition_type = match args.get(i).map(|s| s.as_str()) {
                    Some("smooth") => TransitionType::Smooth,
                    Some("instant") => TransitionType::Instant,
                    Some("slide") => TransitionType::Slide,
                    Some("fluid") => TransitionType::Fluid,
                    _ => {
                        eprintln!("Error: --transition-type must be 'smooth', 'instant', 'slide', or 'fluid'");
                        return None;
                    }
                };
            }
            "--transition-duration" => {
                i += 1;
                config.transition_duration = args.get(i)?.parse().ok()?;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                print_usage();
                return None;
            }
        }
        i += 1;
    }

    Some(config)
}

pub fn init_example_presentation(path: &std::path::Path) {
    use rand::Rng;

    struct SlideDef {
        num: u32,
        name: &'static str,
        notes: &'static str,
    }

    struct Chapter {
        num: u32,
        name: &'static str,
        slides: Vec<SlideDef>,
    }

    let chapters = vec![
        Chapter {
            num: 1,
            name: "Getting Started",
            slides: vec![
                SlideDef {
                    num: 1,
                    name: "Overview",
                    notes: "Welcome to this example presentation.\n\nThis slide deck demonstrates the RS-Vulkan presentation viewer.\nEach slide is a PNG image with a transparent background and randomly placed rectangles.",
                },
                SlideDef {
                    num: 2,
                    name: "Installation",
                    notes: "To install RS-Vulkan:\n\n1. Clone the repository\n2. Run `cargo build --release`\n3. Run `rs-vulkan <slides-folder>`\n\nMake sure you have Vulkan drivers installed on your system.",
                },
                SlideDef {
                    num: 3,
                    name: "Hello World",
                    notes: "Create your first presentation:\n\n```\nrs-vulkan init my-presentation\nrs-vulkan my-presentation\n```\n\nNavigate with arrow keys and Escape to quit.",
                },
            ],
        },
        Chapter {
            num: 2,
            name: "Advanced Topics",
            slides: vec![
                SlideDef {
                    num: 1,
                    name: "Configuration",
                    notes: "Customize the viewing experience:\n\n- `--transition-duration`: Transition timing\n\nDefault values work well for most presentations.",
                },
                SlideDef {
                    num: 2,
                    name: "Deployment",
                    notes: "Tips for presentations:\n\n- Use consistent slide dimensions\n- PNG files support transparency\n- Use `chapter_slide.png` naming convention\n- Add presenter notes via `presenter_notes.md`",
                },
            ],
        },
        Chapter {
            num: 3,
            name: "Conclusion",
            slides: vec![
                SlideDef {
                    num: 1,
                    name: "Summary",
                    notes: "RS-Vulkan features:\n\n- Hardware-accelerated rendering via Vulkan\n- Smooth transitions with compute-shader blur and feedback\n- Presenter notes support\n- Chapter-based navigation\n- Fullscreen presentation mode",
                },
                SlideDef {
                    num: 2,
                    name: "Next Steps",
                    notes: "Ideas for extending:\n\n- Add video slide support\n- Implement smooth scrolling\n- Add remote control support\n- Build a slide editor UI",
                },
            ],
        },
    ];

    std::fs::create_dir_all(path).expect("Failed to create presentation directory");

    let width = 1920u32;
    let height = 1080u32;
    let mut rng = rand::thread_rng();

    let mut notes_content = String::new();

    for ch in &chapters {
        notes_content.push_str(&format!("# {}: {}\n", ch.num, ch.name));
        for slide in &ch.slides {
            let filename = format!("{}_{}.png", ch.num, slide.num);
            let filepath = path.join(&filename);

            let mut img = image::RgbaImage::new(width, height);

            let rect_count = rng.gen_range(5..=12);
            for _ in 0..rect_count {
                let rx = rng.gen_range(0..width.saturating_sub(50).max(1));
                let ry = rng.gen_range(0..height.saturating_sub(50).max(1));
                let max_w = (width - rx).min(400);
                let max_h = (height - ry).min(300);
                let rw = rng.gen_range(50..=max_w.max(50));
                let rh = rng.gen_range(50..=max_h.max(50));
                let rr = rng.gen_range(30..=220);
                let rg = rng.gen_range(30..=220);
                let rb = rng.gen_range(30..=220);
                let ra = rng.gen_range(60..=200);

                for py in ry..ry + rh {
                    for px in rx..rx + rw {
                        img.put_pixel(px, py, image::Rgba([rr, rg, rb, ra]));
                    }
                }
            }

            img.save(&filepath).expect("Failed to save slide image");

            notes_content.push_str(&format!("## {}_{}: {}\n{}\n", ch.num, slide.num, slide.name, slide.notes));
        }
    }

    let notes_path = path.join("presenter_notes.md");
    std::fs::write(&notes_path, notes_content).expect("Failed to write presenter notes");
}

#[allow(dead_code)]
pub struct GpuResources {
    pub queue: Arc<vulkano::device::Queue>,
    pub descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    pub command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    pub swapchain: Arc<Swapchain>,
    pub swapchain_images: Vec<Arc<Image>>,
    pub swapchain_image_views: Vec<Arc<ImageView>>,

    // Direct rendering pipeline (for Instant/Slide and non-transitioning)
    pub direct_pipeline: Arc<vulkano::pipeline::graphics::GraphicsPipeline>,
    pub direct_pipeline_layout: Arc<PipelineLayout>,

    // Smooth transition pipelines
    pub blur_h_pipeline: Arc<ComputePipeline>,
    pub blur_v_pipeline: Arc<ComputePipeline>,
    pub blur_pipeline_layout: Arc<PipelineLayout>,
    pub present_pipeline: Arc<vulkano::pipeline::graphics::GraphicsPipeline>,
    pub present_pipeline_layout: Arc<PipelineLayout>,

    // Slides sampler descriptor set (shared by direct and present pipelines)
    pub slides_descriptor_set: Arc<DescriptorSet>,

    // Feedback texture (single buffer, blurred in-place via ping)
    pub feedback: Arc<Image>,
    pub feedback_view: Arc<ImageView>,
    pub ping_image: Arc<Image>,
    pub ping_view: Arc<ImageView>,
    pub sampler: Arc<Sampler>,

    // Present descriptor set (renders feedback to swapchain)
    pub present_descriptor_set: Arc<DescriptorSet>,

    // Blur descriptor sets
    pub blur_h_descriptor_set: Arc<DescriptorSet>, // feedback → ping
    pub blur_v_descriptor_set: Arc<DescriptorSet>, // ping → feedback

    // Stable fluids simulation resources
    pub velocity: Arc<Image>,
    pub velocity_view: Arc<ImageView>,
    pub velocity_ping: Arc<Image>,
    pub velocity_ping_view: Arc<ImageView>,
    pub pressure: Arc<Image>,
    pub pressure_view: Arc<ImageView>,
    pub pressure_ping: Arc<Image>,
    pub pressure_ping_view: Arc<ImageView>,
    pub divergence: Arc<Image>,
    pub divergence_view: Arc<ImageView>,

    // Fluid pipeline layouts and descriptor set layouts
    pub fluid_2binding_ds_layout: Arc<DescriptorSetLayout>,
    pub fluid_2binding_layout: Arc<PipelineLayout>,
    pub fluid_3binding_ds_layout: Arc<DescriptorSetLayout>,
    pub fluid_3binding_layout: Arc<PipelineLayout>,

    // Fluid compute pipelines
    pub fluid_advect_pipeline: Arc<ComputePipeline>,
    pub fluid_divergence_pipeline: Arc<ComputePipeline>,
    pub fluid_jacobi_pipeline: Arc<ComputePipeline>,
    pub fluid_gradient_subtract_pipeline: Arc<ComputePipeline>,
    pub fluid_init_velocity_pipeline: Arc<ComputePipeline>,

    // Fluid descriptor sets (pre-built for all simulation steps)
    pub fluid_advect_velocity_ds: Arc<DescriptorSet>,    // velocity, velocity, velocity_ping
    pub fluid_advect_density_h_ds: Arc<DescriptorSet>,    // feedback, velocity, ping
    pub fluid_advect_density_v_ds: Arc<DescriptorSet>,    // ping, velocity, feedback
    pub fluid_divergence_ds: Arc<DescriptorSet>,          // velocity_ping, divergence
    pub fluid_jacobi_h_ds: Arc<DescriptorSet>,            // divergence, pressure, pressure_ping
    pub fluid_jacobi_v_ds: Arc<DescriptorSet>,            // divergence, pressure_ping, pressure
    pub fluid_gradient_subtract_ds: Arc<DescriptorSet>,   // pressure, velocity_ping, velocity
    pub fluid_init_velocity_ds: Arc<DescriptorSet>,       // feedback, velocity

    // Blend descriptor set and pipeline (slide → feedback, first frame only)
    pub blend_pipeline: Arc<ComputePipeline>,
    pub blend_pipeline_layout: Arc<PipelineLayout>,
    pub blend_descriptor_set: Arc<DescriptorSet>,

    pub window: Arc<winit::window::Window>,
}

pub struct App {
    pub resources: GpuResources,
    pub collection: texture::SlideCollection,
    pub current_layer: usize,
    pub target_layer: usize,
    pub previous_layer: usize,
    pub transition_time: f32,
    pub is_transitioning: bool,
    pub config: AppConfig,
    pub last_frame: Instant,
    pub transition_direction: (f32, f32),
    pub transition_blended: bool,
    previous_frame: Option<Box<dyn GpuFuture>>,
}

fn required_instance_extensions() -> InstanceExtensions {
    let mut e = InstanceExtensions {
        khr_surface: true,
        khr_get_physical_device_properties2: true,
        ..InstanceExtensions::empty()
    };
    if cfg!(target_os = "macos") {
        e.ext_metal_surface = true;
        e.khr_portability_enumeration = true;
    }
    e
}

fn required_device_extensions(
    phys_device: &vulkano::device::physical::PhysicalDevice,
) -> DeviceExtensions {
    let mut e = DeviceExtensions {
        khr_swapchain: true,
        khr_dynamic_rendering: true,
        ..DeviceExtensions::empty()
    };
    if cfg!(target_os = "macos") && phys_device.supported_extensions().khr_portability_subset {
        e.khr_portability_subset = true;
    }
    e
}

fn create_texture_array(
    device: Arc<Device>,
    allocator: Arc<StandardMemoryAllocator>,
    queue: Arc<vulkano::device::Queue>,
    cmd_allocator: Arc<StandardCommandBufferAllocator>,
    paths: &[std::path::PathBuf],
) -> Result<(Arc<Image>, Arc<ImageView>, Arc<Sampler>), Box<dyn std::error::Error>> {
    let first_img = image::open(&paths[0])
        .map_err(|e| format!("open first image: {e}"))?
        .to_rgba8();
    let (width, height) = first_img.dimensions();
    let num_layers = paths.len() as u32;

    let mut all_pixels: Vec<u16> = Vec::new();
    for path in paths {
        let img = image::open(path)
            .map_err(|e| format!("open image {}: {e}", path.display()))?
            .to_rgba8();
        if img.dimensions() != (width, height) {
            return Err(format!(
                "Image '{}' is {}x{}, expected {}x{}",
                path.display(),
                img.width(),
                img.height(),
                width,
                height
            )
            .into());
        }
        for pixel in img.pixels() {
            for &ch in pixel.0.iter() {
                let f = ch as f32 / 255.0;
                all_pixels.push(f16::from_f32(f).to_bits());
            }
        }
    }

    let pixel_count = all_pixels.len() as u64;

    let image = Image::new(
        allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R16G16B16A16_SFLOAT,
            extent: [width, height, 1],
            array_layers: num_layers,
            mip_levels: 1,
            usage: ImageUsage::TRANSFER_DST | ImageUsage::SAMPLED,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .map_err(|e| format!("texture image: {e}"))?;

    let staging = Buffer::new_slice::<u16>(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        pixel_count,
    )
    .map_err(|e| format!("staging buffer: {e}"))?;

    {
        let mut staging_content = staging.write()?;
        staging_content.copy_from_slice(&all_pixels);
    }

    let mut builder = AutoCommandBufferBuilder::primary(
        cmd_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .map_err(|e| format!("cmd builder: {e}"))?;

    builder
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(staging, image.clone()))
        .map_err(|e| format!("copy buffer to image: {e}"))?;
    let staging_cb = builder
        .build()
        .map_err(|e| format!("build staging cb: {e}"))?;
    staging_cb
        .execute(queue.clone())
        .map_err(|e| format!("execute staging: {e}"))?
        .then_signal_fence_and_flush()
        .map_err(|e| format!("signal fence: {e}"))?
        .wait(None)
        .map_err(|e| format!("wait staging: {e}"))?;

    let view = ImageView::new(
        image.clone(),
        ImageViewCreateInfo {
            view_type: ImageViewType::Dim2dArray,
            format: Format::R16G16B16A16_SFLOAT,
            subresource_range: ImageSubresourceRange {
                aspects: ImageAspects::COLOR,
                mip_levels: 0..1,
                array_layers: 0..num_layers,
            },
            ..Default::default()
        },
    )
    .map_err(|e| format!("texture image view: {e}"))?;

    let sampler = Sampler::new(
        device.clone(),
        SamplerCreateInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mipmap_mode: SamplerMipmapMode::Linear,
            address_mode: [
                SamplerAddressMode::ClampToEdge,
                SamplerAddressMode::ClampToEdge,
                SamplerAddressMode::ClampToEdge,
            ],
            ..Default::default()
        },
    )
    .map_err(|e| format!("sampler: {e}"))?;

    Ok((image, view, sampler))
}

fn create_feedback_image(
    allocator: &Arc<StandardMemoryAllocator>,
    extent: [u32; 2],
) -> Result<Arc<Image>, Box<dyn std::error::Error>> {
    let img = Image::new(
        allocator.clone() as Arc<dyn vulkano::memory::allocator::MemoryAllocator>,
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R16G16B16A16_SFLOAT,
            extent: [extent[0], extent[1], 1],
            array_layers: 1,
            mip_levels: 1,
            usage: ImageUsage::COLOR_ATTACHMENT
                | ImageUsage::SAMPLED
                | ImageUsage::STORAGE
                | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )?;
    Ok(img)
}

fn create_feedback_view(
    image: Arc<Image>,
) -> Result<Arc<ImageView>, Box<dyn std::error::Error>> {
    Ok(ImageView::new(
        image,
        ImageViewCreateInfo {
            view_type: ImageViewType::Dim2d,
            format: Format::R16G16B16A16_SFLOAT,
            subresource_range: ImageSubresourceRange {
                aspects: ImageAspects::COLOR,
                mip_levels: 0..1,
                array_layers: 0..1,
            },
            ..Default::default()
        },
    )?)
}

pub fn create_app(
    config: AppConfig,
    event_loop: &winit::event_loop::ActiveEventLoop,
) -> Result<App, Box<dyn std::error::Error>> {
    let (slide_keys, slide_meta, slide_paths) =
        texture::load_slide_directory(&config.slides_path)
            .map_err(|e| format!("Failed to load slides: {e}"))?;

    let collection = texture::SlideCollection {
        slides: slide_keys,
        metadata: slide_meta,
    };

    let window_attrs = winit::window::Window::default_attributes()
        .with_title("RS-Vulkan Slides")
        .with_inner_size(winit::dpi::PhysicalSize::new(1920, 1080))
        .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
    let window = Arc::new(event_loop.create_window(window_attrs)?);

    let library = VulkanLibrary::new()?;

    let instance_extensions = required_instance_extensions();
    let instance = Instance::new(
        library,
        InstanceCreateInfo {
            enabled_extensions: instance_extensions,
            ..Default::default()
        },
    )?;

    let surface = Surface::from_window(instance.clone(), window.clone())?;

    let physical_devices = instance.enumerate_physical_devices()?;
    let phys_device = physical_devices
        .filter(|p| {
            p.supported_extensions().khr_swapchain
                && p.supported_extensions().khr_dynamic_rendering
        })
        .min_by_key(|p| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            _ => 4,
        })
        .ok_or("No suitable physical device found (needs swapchain + dynamic rendering)")?;

    let queue_family_index = phys_device
        .queue_family_properties()
        .iter()
        .position(|qf| {
            qf.queue_flags.contains(QueueFlags::GRAPHICS)
        })
        .and_then(|idx| {
            phys_device
                .surface_support(idx as u32, &surface)
                .ok()
                .and_then(|supported| if supported { Some(idx as u32) } else { None })
        })
        .ok_or("No queue family with graphics + present")?;

    let device_extensions = required_device_extensions(&phys_device);

    let (device, mut queues) = Device::new(
        phys_device,
        DeviceCreateInfo {
            queue_create_infos: vec![QueueCreateInfo {
                queue_family_index,
                ..Default::default()
            }],
            enabled_extensions: device_extensions,
            enabled_features: DeviceFeatures {
                dynamic_rendering: true,
                ..DeviceFeatures::default()
            },
            ..Default::default()
        },
    )
    .map_err(|e| format!("Device::new: {e}"))?;

    let queue = queues.next().ok_or("No queues created")?;

    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
    let cmd_allocator = Arc::new(StandardCommandBufferAllocator::new(
        device.clone(),
        Default::default(),
    ));

    let (format, _) = choose_swapchain_format(device.clone(), surface.clone())
        .map_err(|e| format!("choose_swapchain_format: {e}"))?;
    let (swapchain, swapchain_images) =
        create_swapchain(device.clone(), surface.clone(), format)
            .map_err(|e| format!("create_swapchain: {e}"))?;

    let swapchain_image_views: Vec<Arc<ImageView>> = swapchain_images
        .iter()
        .map(|img| {
            ImageView::new(
                img.clone(),
                ImageViewCreateInfo {
                    format,
                    view_type: ImageViewType::Dim2d,
                    subresource_range: ImageSubresourceRange {
                        aspects: ImageAspects::COLOR,
                        mip_levels: 0..1,
                        array_layers: 0..1,
                    },
                    ..Default::default()
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("swapchain image views: {e}"))?;

    let (_, texture_view, sampler) = create_texture_array(
        device.clone(),
        memory_allocator.clone(),
        queue.clone(),
        cmd_allocator.clone(),
        &slide_paths,
    )
    .map_err(|e| format!("create_texture_array: {e}"))?;

    let scene_extent = swapchain_images[0].extent();
    let extent2 = [scene_extent[0], scene_extent[1]];

    // Create single feedback texture and intermediate ping buffer
    let feedback_img = create_feedback_image(&memory_allocator, extent2)?;
    let feedback_view = create_feedback_view(feedback_img.clone())?;

    let ping_image = Image::new(
        memory_allocator.clone() as Arc<dyn vulkano::memory::allocator::MemoryAllocator>,
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R16G16B16A16_SFLOAT,
            extent: [scene_extent[0], scene_extent[1], 1],
            array_layers: 1,
            mip_levels: 1,
            usage: ImageUsage::STORAGE | ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )?;
    let ping_view = ImageView::new(
        ping_image.clone(),
        ImageViewCreateInfo {
            view_type: ImageViewType::Dim2d,
            format: Format::R16G16B16A16_SFLOAT,
            subresource_range: ImageSubresourceRange {
                aspects: ImageAspects::COLOR,
                mip_levels: 0..1,
                array_layers: 0..1,
            },
            ..Default::default()
        },
    )?;

    // Helper: create rgba16f storage image for fluid simulation
    let make_fluid_image = |allocator: &Arc<StandardMemoryAllocator>, extent: [u32; 2]| -> Result<(Arc<Image>, Arc<ImageView>), Box<dyn std::error::Error>> {
        let img = Image::new(
            allocator.clone() as Arc<dyn vulkano::memory::allocator::MemoryAllocator>,
            ImageCreateInfo {
                image_type: ImageType::Dim2d,
                format: Format::R16G16B16A16_SFLOAT,
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
            ImageViewCreateInfo {
                view_type: ImageViewType::Dim2d,
                format: Format::R16G16B16A16_SFLOAT,
                subresource_range: ImageSubresourceRange {
                    aspects: ImageAspects::COLOR,
                    mip_levels: 0..1,
                    array_layers: 0..1,
                },
                ..Default::default()
            },
        )?;
        Ok((img, view))
    };

    let (velocity, velocity_view) = make_fluid_image(&memory_allocator, extent2)?;
    let (velocity_ping, velocity_ping_view) = make_fluid_image(&memory_allocator, extent2)?;
    let (pressure, pressure_view) = make_fluid_image(&memory_allocator, extent2)?;
    let (pressure_ping, pressure_ping_view) = make_fluid_image(&memory_allocator, extent2)?;
    let (divergence, divergence_view) = make_fluid_image(&memory_allocator, extent2)?;

    // Clear feedback and ping buffers so the IIR feedback loop starts
    // from black instead of undefined GPU memory.
    {
        let mut clear_builder = AutoCommandBufferBuilder::primary(
            cmd_allocator.clone(),
            queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .map_err(|e| format!("clear cmd builder: {e}"))?;
        clear_builder
            .clear_color_image(ClearColorImageInfo::image(feedback_img.clone()))
            .map_err(|e| format!("clear feedback: {e}"))?;
        clear_builder
            .clear_color_image(ClearColorImageInfo::image(ping_image.clone()))
            .map_err(|e| format!("clear ping: {e}"))?;
        clear_builder
            .clear_color_image(ClearColorImageInfo::image(velocity.clone()))
            .map_err(|e| format!("clear velocity: {e}"))?;
        clear_builder
            .clear_color_image(ClearColorImageInfo::image(velocity_ping.clone()))
            .map_err(|e| format!("clear velocity_ping: {e}"))?;
        clear_builder
            .clear_color_image(ClearColorImageInfo::image(pressure.clone()))
            .map_err(|e| format!("clear pressure: {e}"))?;
        clear_builder
            .clear_color_image(ClearColorImageInfo::image(pressure_ping.clone()))
            .map_err(|e| format!("clear pressure_ping: {e}"))?;
        clear_builder
            .clear_color_image(ClearColorImageInfo::image(divergence.clone()))
            .map_err(|e| format!("clear divergence: {e}"))?;
        clear_builder
            .build()
            .map_err(|e| format!("build clear cb: {e}"))?
            .execute(queue.clone())
            .map_err(|e| format!("execute clear: {e}"))?
            .then_signal_fence_and_flush()
            .map_err(|e| format!("flush clear: {e}"))?
            .wait(None)
            .map_err(|e| format!("wait clear: {e}"))?;
    }

    // Shared descriptor set for slides
    let slides_descriptor_set_layout = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([(
                0u32,
                DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::FRAGMENT,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::CombinedImageSampler)
                },
            )]),
            ..Default::default()
        },
    )?;

    let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
        device.clone(),
        Default::default(),
    ));

    let slides_descriptor_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        slides_descriptor_set_layout.clone(),
        [WriteDescriptorSet::image_view_sampler(
            0,
            texture_view.clone(),
            sampler.clone(),
        )],
        [],
    )
    .map_err(|e| format!("slides descriptor_set: {e}"))?;

    // Direct pipeline layout (for Instant/Slide and non-transitioning)
    let direct_pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![slides_descriptor_set_layout.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: vulkano::shader::ShaderStages::FRAGMENT,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    let vs_module = vs::load(device.clone())
        .map_err(|e| format!("vs load: {e}"))?;
    let cs_blur_module = cs_blur::load(device.clone())
        .map_err(|e| format!("cs_blur load: {e}"))?;
    let cs_blur_v_module = cs_blur_v::load(device.clone())
        .map_err(|e| format!("cs_blur_v load: {e}"))?;
    let fs_present_module = fs_present::load(device.clone())
        .map_err(|e| format!("fs_present load: {e}"))?;
    let fs_direct_module = fs_direct::load(device.clone())
        .map_err(|e| format!("fs_direct load: {e}"))?;
    let cs_blend_slide_module = cs_blend_slide::load(device.clone())
        .map_err(|e| format!("cs_blend_slide load: {e}"))?;
    let cs_fluid_advect_module = cs_fluid_advect::load(device.clone())
        .map_err(|e| format!("cs_fluid_advect load: {e}"))?;
    let cs_fluid_divergence_module = cs_fluid_divergence::load(device.clone())
        .map_err(|e| format!("cs_fluid_divergence load: {e}"))?;
    let cs_fluid_jacobi_module = cs_fluid_jacobi::load(device.clone())
        .map_err(|e| format!("cs_fluid_jacobi load: {e}"))?;
    let cs_fluid_gradient_subtract_module = cs_fluid_gradient_subtract::load(device.clone())
        .map_err(|e| format!("cs_fluid_gradient_subtract load: {e}"))?;
    let cs_fluid_init_velocity_module = cs_fluid_init_velocity::load(device.clone())
        .map_err(|e| format!("cs_fluid_init_velocity load: {e}"))?;

    let vs_entry = vs_module
        .entry_point("main")
        .ok_or("Vertex shader entry point 'main' not found")?;
    let cs_blur_entry = cs_blur_module
        .entry_point("main")
        .ok_or("Cs_blur compute shader entry point 'main' not found")?;
    let cs_blur_v_entry = cs_blur_v_module
        .entry_point("main")
        .ok_or("Cs_blur_v compute shader entry point 'main' not found")?;
    let cs_blend_slide_entry = cs_blend_slide_module
        .entry_point("main")
        .ok_or("Cs_blend_slide compute shader entry point 'main' not found")?;
    let cs_fluid_advect_entry = cs_fluid_advect_module
        .entry_point("main")
        .ok_or("Cs_fluid_advect compute shader entry point 'main' not found")?;
    let cs_fluid_divergence_entry = cs_fluid_divergence_module
        .entry_point("main")
        .ok_or("Cs_fluid_divergence compute shader entry point 'main' not found")?;
    let cs_fluid_jacobi_entry = cs_fluid_jacobi_module
        .entry_point("main")
        .ok_or("Cs_fluid_jacobi compute shader entry point 'main' not found")?;
    let cs_fluid_gradient_subtract_entry = cs_fluid_gradient_subtract_module
        .entry_point("main")
        .ok_or("Cs_fluid_gradient_subtract compute shader entry point 'main' not found")?;
    let cs_fluid_init_velocity_entry = cs_fluid_init_velocity_module
        .entry_point("main")
        .ok_or("Cs_fluid_init_velocity compute shader entry point 'main' not found")?;
    let fs_present_entry = fs_present_module
        .entry_point("main")
        .ok_or("Fs_present fragment shader entry point 'main' not found")?;
    let fs_direct_entry = fs_direct_module
        .entry_point("main")
        .ok_or("Fs_direct fragment shader entry point 'main' not found")?;

    // Present pipeline (composites slide over blurred feedback for swapchain)
    let present_descriptor_set_layout = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([
                (0u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::FRAGMENT,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::CombinedImageSampler)
                }),
            ]),
            ..Default::default()
        },
    )?;

    let present_pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![
                present_descriptor_set_layout.clone(),
                slides_descriptor_set_layout.clone(),
            ],
            push_constant_ranges: vec![PushConstantRange {
                stages: vulkano::shader::ShaderStages::FRAGMENT,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    // Create present descriptor set (reads feedback, outputs to swapchain)
    let present_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        present_descriptor_set_layout.clone(),
        [
            WriteDescriptorSet::image_view_with_layout_sampler(
                0,
                DescriptorImageViewInfo {
                    image_view: feedback_view.clone(),
                    image_layout: ImageLayout::General,
                },
                sampler.clone(),
            ),
        ],
        [],
    )
    .map_err(|e| format!("present descriptor_set: {e}"))?;

    let present_pipeline =
        vulkano::pipeline::graphics::GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: smallvec![
                    PipelineShaderStageCreateInfo::new(vs_entry.clone()),
                    PipelineShaderStageCreateInfo::new(fs_present_entry),
                ],
                vertex_input_state: Some(VertexInputState::default()),
                input_assembly_state: Some(InputAssemblyState {
                    topology: PrimitiveTopology::TriangleList,
                    ..Default::default()
                }),
                dynamic_state: [
                    DynamicState::Viewport,
                    DynamicState::Scissor,
                ].into_iter().collect(),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState {
                    cull_mode: CullMode::None,
                    ..Default::default()
                }),
                multisample_state: Some(MultisampleState::default()),
                depth_stencil_state: None,
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    1,
                    ColorBlendAttachmentState::default(),
                )),
                subpass: Some(PipelineSubpassType::BeginRendering(
                    PipelineRenderingCreateInfo {
                        color_attachment_formats: vec![Some(format)],
                        ..Default::default()
                    },
                )),
                ..GraphicsPipelineCreateInfo::layout(present_pipeline_layout.clone())
            },
        )
        .map_err(|e| format!("present_pipeline: {e}"))?;

    // Direct pipeline (Instant / Slide / non-transitioning)
    let direct_pipeline =
        vulkano::pipeline::graphics::GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: smallvec![
                    PipelineShaderStageCreateInfo::new(vs_entry),
                    PipelineShaderStageCreateInfo::new(fs_direct_entry),
                ],
                vertex_input_state: Some(VertexInputState::default()),
                input_assembly_state: Some(InputAssemblyState {
                    topology: PrimitiveTopology::TriangleList,
                    ..Default::default()
                }),
                dynamic_state: [
                    DynamicState::Viewport,
                    DynamicState::Scissor,
                ].into_iter().collect(),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState {
                    cull_mode: CullMode::None,
                    ..Default::default()
                }),
                multisample_state: Some(MultisampleState::default()),
                depth_stencil_state: None,
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    1,
                    ColorBlendAttachmentState::default(),
                )),
                subpass: Some(PipelineSubpassType::BeginRendering(
                    PipelineRenderingCreateInfo {
                        color_attachment_formats: vec![Some(format)],
                        ..Default::default()
                    },
                )),
                ..GraphicsPipelineCreateInfo::layout(direct_pipeline_layout.clone())
            },
        )
        .map_err(|e| format!("direct_pipeline: {e}"))?;

    // Compute blur pipeline layout
    let blur_descriptor_set_layout = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([
                (0u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
                (1u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
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
                stages: vulkano::shader::ShaderStages::COMPUTE,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    // Create blur descriptor sets (feedback → ping, ping → feedback)
    let make_blur_set = |src: Arc<ImageView>, dst: Arc<ImageView>| -> Result<Arc<DescriptorSet>, Box<dyn std::error::Error>> {
        DescriptorSet::new(
            descriptor_set_allocator.clone(),
            blur_descriptor_set_layout.clone(),
            [
                WriteDescriptorSet::image_view_with_layout(
                    0,
                    DescriptorImageViewInfo {
                        image_view: src,
                        image_layout: ImageLayout::General,
                    },
                ),
                WriteDescriptorSet::image_view_with_layout(
                    1,
                    DescriptorImageViewInfo {
                        image_view: dst,
                        image_layout: ImageLayout::General,
                    },
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
    )
    .map_err(|e| format!("blur_h_pipeline: {e}"))?;

    let blur_v_pipeline = ComputePipeline::new(
        device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_blur_v_entry),
            blur_pipeline_layout.clone(),
        ),
    )
    .map_err(|e| format!("blur_v_pipeline: {e}"))?;

    // ------------------------------------------------------------------
    // Stable fluids simulation pipelines
    // ------------------------------------------------------------------

    // 2-binding DS layout (used by: divergence, init_velocity)
    let fluid_2binding_ds_layout = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([
                (0u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
                (1u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
            ]),
            ..Default::default()
        },
    )?;

    let fluid_2binding_layout = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![fluid_2binding_ds_layout.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: vulkano::shader::ShaderStages::COMPUTE,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    // 3-binding DS layout (used by: advect, jacobi, gradient_subtract)
    let fluid_3binding_ds_layout = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([
                (0u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
                (1u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
                (2u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
            ]),
            ..Default::default()
        },
    )?;

    let fluid_3binding_layout = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![fluid_3binding_ds_layout.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: vulkano::shader::ShaderStages::COMPUTE,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    let make_2binding_set = |b0: Arc<ImageView>, b1: Arc<ImageView>| -> Result<Arc<DescriptorSet>, Box<dyn std::error::Error>> {
        DescriptorSet::new(
            descriptor_set_allocator.clone(),
            fluid_2binding_ds_layout.clone(),
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

    let make_3binding_set = |b0: Arc<ImageView>, b1: Arc<ImageView>, b2: Arc<ImageView>| -> Result<Arc<DescriptorSet>, Box<dyn std::error::Error>> {
        DescriptorSet::new(
            descriptor_set_allocator.clone(),
            fluid_3binding_ds_layout.clone(),
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

    // Fluid compute pipelines
    let fluid_advect_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_fluid_advect_entry),
            fluid_3binding_layout.clone(),
        ),
    )
    .map_err(|e| format!("fluid_advect_pipeline: {e}"))?;

    let fluid_divergence_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_fluid_divergence_entry),
            fluid_2binding_layout.clone(),
        ),
    )
    .map_err(|e| format!("fluid_divergence_pipeline: {e}"))?;

    let fluid_jacobi_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_fluid_jacobi_entry),
            fluid_3binding_layout.clone(),
        ),
    )
    .map_err(|e| format!("fluid_jacobi_pipeline: {e}"))?;

    let fluid_gradient_subtract_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_fluid_gradient_subtract_entry),
            fluid_3binding_layout.clone(),
        ),
    )
    .map_err(|e| format!("fluid_gradient_subtract_pipeline: {e}"))?;

    let fluid_init_velocity_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_fluid_init_velocity_entry),
            fluid_2binding_layout.clone(),
        ),
    )
    .map_err(|e| format!("fluid_init_velocity_pipeline: {e}"))?;

    // Fluid descriptor sets (pre-built for all simulation steps)
    // Advection: velocity field → self-advect to velocity_ping
    let fluid_advect_velocity_ds = make_3binding_set(
        velocity_view.clone(), velocity_view.clone(), velocity_ping_view.clone(),
    )?;
    // Advection: density feedback → ping (using velocity field)
    let fluid_advect_density_h_ds = make_3binding_set(
        feedback_view.clone(), velocity_view.clone(), ping_view.clone(),
    )?;
    // Advection: density ping → feedback (using velocity field)
    let fluid_advect_density_v_ds = make_3binding_set(
        ping_view.clone(), velocity_view.clone(), feedback_view.clone(),
    )?;
    // Divergence: velocity_ping → divergence
    let fluid_divergence_ds = make_2binding_set(
        velocity_ping_view.clone(), divergence_view.clone(),
    )?;
    // Jacobi H: divergence, pressure → pressure_ping
    let fluid_jacobi_h_ds = make_3binding_set(
        divergence_view.clone(), pressure_view.clone(), pressure_ping_view.clone(),
    )?;
    // Jacobi V: divergence, pressure_ping → pressure
    let fluid_jacobi_v_ds = make_3binding_set(
        divergence_view.clone(), pressure_ping_view.clone(), pressure_view.clone(),
    )?;
    // Gradient subtract: pressure, velocity_ping → velocity
    let fluid_gradient_subtract_ds = make_3binding_set(
        pressure_view.clone(), velocity_ping_view.clone(), velocity_view.clone(),
    )?;
    // Init velocity: feedback → velocity
    let fluid_init_velocity_ds = make_2binding_set(
        feedback_view.clone(), velocity_view.clone(),
    )?;

    // Blend pipeline: blends current slide into feedback buffer (first frame only)
    let blend_descriptor_set_layout = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([
                (0u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::StorageImage)
                }),
                (1u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::COMPUTE,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::CombinedImageSampler)
                }),
            ]),
            ..Default::default()
        },
    )?;

    let blend_pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![blend_descriptor_set_layout.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: vulkano::shader::ShaderStages::COMPUTE,
                offset: 0,
                size: 20,
            }],
            ..Default::default()
        },
    )?;

    let blend_descriptor_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        blend_descriptor_set_layout.clone(),
        [
            WriteDescriptorSet::image_view_with_layout(
                0,
                DescriptorImageViewInfo {
                    image_view: feedback_view.clone(),
                    image_layout: ImageLayout::General,
                },
            ),
            WriteDescriptorSet::image_view_sampler(
                1,
                texture_view.clone(),
                sampler.clone(),
            ),
        ],
        [],
    )
    .map_err(|e| format!("blend descriptor_set: {e}"))?;

    let blend_pipeline = ComputePipeline::new(
        device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(
            PipelineShaderStageCreateInfo::new(cs_blend_slide_entry),
            blend_pipeline_layout.clone(),
        ),
    )
    .map_err(|e| format!("blend_pipeline: {e}"))?;

    if let Some(meta) = collection.meta(0) {
        println!("{}", texture::format_slide_display(meta, 0, collection.len()));
    }

    Ok(App {
        resources: GpuResources {
            queue,
            descriptor_set_allocator,
            command_buffer_allocator: cmd_allocator,
            swapchain,
            swapchain_images,
            swapchain_image_views,
            direct_pipeline,
            direct_pipeline_layout,
            blur_h_pipeline,
            blur_v_pipeline,
            blur_pipeline_layout,
            velocity,
            velocity_view,
            velocity_ping,
            velocity_ping_view,
            pressure,
            pressure_view,
            pressure_ping,
            pressure_ping_view,
            divergence,
            divergence_view,
            fluid_2binding_ds_layout,
            fluid_2binding_layout,
            fluid_3binding_ds_layout,
            fluid_3binding_layout,
            fluid_advect_pipeline,
            fluid_divergence_pipeline,
            fluid_jacobi_pipeline,
            fluid_gradient_subtract_pipeline,
            fluid_init_velocity_pipeline,
            fluid_advect_velocity_ds,
            fluid_advect_density_h_ds,
            fluid_advect_density_v_ds,
            fluid_divergence_ds,
            fluid_jacobi_h_ds,
            fluid_jacobi_v_ds,
            fluid_gradient_subtract_ds,
            fluid_init_velocity_ds,
            present_pipeline,
            present_pipeline_layout,
            slides_descriptor_set,
            feedback: feedback_img,
            feedback_view,
            ping_image,
            ping_view,
            sampler,
            present_descriptor_set: present_set,
            blur_h_descriptor_set: blur_h_set,
            blur_v_descriptor_set: blur_v_set,
            blend_pipeline,
            blend_pipeline_layout,
            blend_descriptor_set,
            window,
        },
        collection,
        current_layer: 0,
        target_layer: 0,
        previous_layer: 0,
        transition_time: 0.0,
        is_transitioning: false,
        transition_blended: false,
        transition_direction: (0.0, 0.0),
        config,
        last_frame: Instant::now(),
        previous_frame: None,
    })
}

fn choose_swapchain_format(
    device: Arc<Device>,
    surface: Arc<Surface>,
) -> Result<(Format, vulkano::swapchain::ColorSpace), Box<dyn std::error::Error>> {
    let formats = device
        .physical_device()
        .surface_formats(&surface, SurfaceInfo::default())?;
    for &(format, color_space) in &formats {
        if format == Format::B8G8R8A8_SRGB {
            return Ok((format, color_space));
        }
    }
    Ok(formats[0])
}

fn choose_present_mode(
    device: &Arc<Device>,
    surface: &Arc<Surface>,
) -> Result<PresentMode, Box<dyn std::error::Error>> {
    let modes = device
        .physical_device()
        .surface_present_modes(surface, SurfaceInfo::default())?;
    for &mode in &modes {
        if mode == PresentMode::Mailbox {
            return Ok(PresentMode::Mailbox);
        }
    }
    Ok(PresentMode::Fifo)
}

fn create_swapchain(
    device: Arc<Device>,
    surface: Arc<Surface>,
    format: Format,
) -> Result<(Arc<Swapchain>, Vec<Arc<Image>>), Box<dyn std::error::Error>> {
    let caps = device
        .physical_device()
        .surface_capabilities(&surface, SurfaceInfo::default())?;
    let extent = caps.current_extent.unwrap_or([1920, 1080]);
    let present_mode = choose_present_mode(&device, &surface)?;

    let (swapchain, images) = Swapchain::new(
        device.clone(),
        surface.clone(),
        SwapchainCreateInfo {
            min_image_count: caps.min_image_count.max(3),
            image_format: format,
            image_extent: extent,
            image_usage: ImageUsage::COLOR_ATTACHMENT,
            present_mode,
            ..Default::default()
        },
    )?;

    Ok((swapchain, images))
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct PushConstants {
    current_layer: i32,
    previous_layer: i32,
    blur_radius: f32,
    slide_offset_x: f32,
    slide_offset_y: f32,
}

impl App {
    pub fn navigate_to(&mut self, target: Option<usize>) {
        let target = match target {
            Some(t) if t < self.collection.len() => t,
            _ => return,
        };

        if self.is_transitioning {
            self.current_layer = self.target_layer;
        }
        self.previous_layer = self.current_layer;
        self.target_layer = target;
        self.current_layer = target;

        if self.config.transition_type == TransitionType::Instant {
            self.transition_time = 0.0;
            self.is_transitioning = false;
        } else {
            self.transition_time = 0.0;
            self.is_transitioning = true;
        }

        if self.config.transition_type != TransitionType::Slide {
            self.transition_direction = (0.0, 0.0);
        }
        self.last_frame = Instant::now();
        self.transition_blended = false;

        if let Some(meta) = self.collection.meta(target) {
            println!(
                "{}",
                texture::format_slide_display(meta, target, self.collection.len())
            );
        }
    }

    pub fn next_slide(&mut self) {
        let target = self.collection.next_slide(self.current_layer);
        if let Some(t) = target {
            let same_chapter =
                self.collection.chapter_of(self.current_layer) == self.collection.chapter_of(t);
            self.transition_direction = if same_chapter {
                (0.0, 1.0)
            } else {
                (1.0, 0.0)
            };
        }
        self.navigate_to(target);
    }

    pub fn prev_slide(&mut self) {
        let target = self.collection.prev_slide(self.current_layer);
        if let Some(t) = target {
            let same_chapter =
                self.collection.chapter_of(self.current_layer) == self.collection.chapter_of(t);
            self.transition_direction = if same_chapter {
                (0.0, -1.0)
            } else {
                (-1.0, 0.0)
            };
        }
        self.navigate_to(target);
    }

    pub fn next_chapter(&mut self) {
        self.transition_direction = (1.0, 0.0);
        self.navigate_to(self.collection.next_chapter(self.current_layer));
    }

    pub fn prev_chapter(&mut self) {
        self.transition_direction = (-1.0, 0.0);
        self.navigate_to(self.collection.prev_chapter(self.current_layer));
    }

    pub fn request_redraw(&self) {
        self.resources.window.request_redraw();
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        if !self.is_transitioning {
            if self.config.transition_type == TransitionType::Smooth
                || self.config.transition_type == TransitionType::Fluid
            {
                self.request_redraw();
            }
            return;
        }

        self.transition_time += dt;

        let end_dur = match self.config.transition_type {
            TransitionType::Slide => self.config.transition_duration,
            _ => self.config.transition_duration,
        };
        if self.transition_time >= end_dur {
            self.is_transitioning = false;
            self.previous_layer = self.current_layer;
        }
    }

    pub fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        drop(self.previous_frame.take());

        let res = &mut self.resources;

        let (image_index, _, acquire_future) =
            vulkano::swapchain::acquire_next_image(res.swapchain.clone(), None)?;

        let blur_radius = match self.config.transition_type {
            TransitionType::Smooth => 20.0,
            TransitionType::Fluid => self.transition_time,
            _ => 0.0,
        };

        let (slide_offset_x, slide_offset_y) = match self.config.transition_type {
            TransitionType::Slide if self.is_transitioning => {
                let u = (self.transition_time / self.config.transition_duration).min(1.0);
                let ease_out = 1.0 - (1.0 - u) * (1.0 - u) * (1.0 - u);
                (
                    self.transition_direction.0 * (1.0 - ease_out),
                    self.transition_direction.1 * (1.0 - ease_out),
                )
            }
            TransitionType::Fluid => {
                (0.8, 0.02) // speed, dissipation
            }
            _ => (0.0, 0.0),
        };

        let pc = PushConstants {
            current_layer: self.current_layer as i32,
            previous_layer: self.previous_layer as i32,
            blur_radius,
            slide_offset_x,
            slide_offset_y,
        };

        let extent = res.swapchain_images[image_index as usize].extent();
        let viewport = Viewport {
            offset: [0.0, 0.0],
            extent: [extent[0] as f32, extent[1] as f32],
            depth_range: 0.0..=1.0,
        };
        let scissor = Scissor {
            offset: [0, 0],
            extent: [extent[0], extent[1]],
        };

        let mut builder = AutoCommandBufferBuilder::primary(
            res.command_buffer_allocator.clone(),
            res.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;

        if self.config.transition_type == TransitionType::Smooth
            || self.config.transition_type == TransitionType::Fluid
        {
            // Pass 0: Blend previous slide into feedback (first frame only)
            if self.is_transitioning && !self.transition_blended {
                builder
                    .bind_pipeline_compute(res.blend_pipeline.clone())?
                    .bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        res.blend_pipeline_layout.clone(),
                        0,
                        res.blend_descriptor_set.clone(),
                    )?
                    .push_constants(res.blend_pipeline_layout.clone(), 0, pc)?;
                unsafe {
                    builder.dispatch([
                        (extent[0] + 15) / 16,
                        (extent[1] + 15) / 16,
                        1,
                    ])?;
                }
                self.transition_blended = true;
            }

            // --- Stable fluids simulation ---
            if self.config.transition_type == TransitionType::Fluid {
                let ext_x = (extent[0] + 15) / 16;
                let ext_y = (extent[1] + 15) / 16;

                // First frame only: init velocity from feedback gradient
                if self.is_transitioning && !self.transition_blended {
                    let init_pc = PushConstants {
                        current_layer: 0, previous_layer: 0,
                        blur_radius: 50.0,   // velocity scale factor
                        slide_offset_x: 1.0, slide_offset_y: 1.0,
                    };
                    builder
                        .bind_pipeline_compute(res.fluid_init_velocity_pipeline.clone())?
                        .bind_descriptor_sets(
                            PipelineBindPoint::Compute,
                            res.fluid_2binding_layout.clone(), 0,
                            res.fluid_init_velocity_ds.clone(),
                        )?
                        .push_constants(res.fluid_2binding_layout.clone(), 0, init_pc)?;
                    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }
                }

                // PC template for advection steps
                let advect_pc = PushConstants {
                    current_layer: 0, previous_layer: 0,
                    blur_radius: 0.5,   // dt (two substeps = 1.0 total per frame)
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
                    blur_radius: 1.0,   // alpha
                    slide_offset_x: 1.0, slide_offset_y: 1.0,
                };

                // 1. Self-advect velocity field: velocity → velocity_ping
                builder
                    .bind_pipeline_compute(res.fluid_advect_pipeline.clone())?
                    .bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        res.fluid_3binding_layout.clone(), 0,
                        res.fluid_advect_velocity_ds.clone(),
                    )?
                    .push_constants(res.fluid_3binding_layout.clone(), 0, advect_pc)?;
                unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

                // 2. Compute divergence of velocity_ping → divergence
                builder
                    .bind_pipeline_compute(res.fluid_divergence_pipeline.clone())?
                    .bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        res.fluid_2binding_layout.clone(), 0,
                        res.fluid_divergence_ds.clone(),
                    )?
                    .push_constants(res.fluid_2binding_layout.clone(), 0, identity_pc)?;
                unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

                // 3. Jacobi iterations: solve pressure Poisson (10 iters)
                for i in 0..10 {
                    let ds = if i % 2 == 0 {
                        &res.fluid_jacobi_h_ds
                    } else {
                        &res.fluid_jacobi_v_ds
                    };
                    builder
                        .bind_pipeline_compute(res.fluid_jacobi_pipeline.clone())?
                        .bind_descriptor_sets(
                            PipelineBindPoint::Compute,
                            res.fluid_3binding_layout.clone(), 0,
                            ds.clone(),
                        )?
                        .push_constants(res.fluid_3binding_layout.clone(), 0, jacobi_pc)?;
                    unsafe { builder.dispatch([ext_x, ext_y, 1])?; }
                }

                // 4. Subtract pressure gradient: project velocity_ping → velocity
                builder
                    .bind_pipeline_compute(res.fluid_gradient_subtract_pipeline.clone())?
                    .bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        res.fluid_3binding_layout.clone(), 0,
                        res.fluid_gradient_subtract_ds.clone(),
                    )?
                    .push_constants(res.fluid_3binding_layout.clone(), 0, identity_pc)?;
                unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

                // 5a. Advect density: feedback → ping
                builder
                    .bind_pipeline_compute(res.fluid_advect_pipeline.clone())?
                    .bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        res.fluid_3binding_layout.clone(), 0,
                        res.fluid_advect_density_h_ds.clone(),
                    )?
                    .push_constants(res.fluid_3binding_layout.clone(), 0, advect_pc)?;
                unsafe { builder.dispatch([ext_x, ext_y, 1])?; }

                // 5b. Advect density: ping → feedback
                builder
                    .bind_pipeline_compute(res.fluid_advect_pipeline.clone())?
                    .bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        res.fluid_3binding_layout.clone(), 0,
                        res.fluid_advect_density_v_ds.clone(),
                    )?
                    .push_constants(res.fluid_3binding_layout.clone(), 0, advect_pc)?;
                unsafe { builder.dispatch([ext_x, ext_y, 1])?; }
            } else {
                // Pass 1: Horizontal blur (feedback → ping)
                builder
                    .bind_pipeline_compute(res.blur_h_pipeline.clone())?
                    .bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        res.blur_pipeline_layout.clone(),
                        0,
                        res.blur_h_descriptor_set.clone(),
                    )?
                    .push_constants(res.blur_pipeline_layout.clone(), 0, pc)?;
                unsafe {
                    builder.dispatch([
                        (extent[0] + 15) / 16,
                        (extent[1] + 15) / 16,
                        1,
                    ])?;
                }

                // Pass 2: Vertical blur (ping → feedback)
                builder
                    .bind_pipeline_compute(res.blur_v_pipeline.clone())?
                    .bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        res.blur_pipeline_layout.clone(),
                        0,
                        res.blur_v_descriptor_set.clone(),
                    )?
                    .push_constants(res.blur_pipeline_layout.clone(), 0, pc)?;
                unsafe {
                    builder.dispatch([
                        (extent[0] + 15) / 16,
                        (extent[1] + 15) / 16,
                        1,
                    ])?;
                }
            }

            // Pass 3: Composite slide over simulated feedback and present to swapchain
            builder
                .begin_rendering(RenderingInfo {
                    color_attachments: vec![Some(RenderingAttachmentInfo {
                        load_op: AttachmentLoadOp::Clear,
                        store_op: AttachmentStoreOp::Store,
                        clear_value: Some(ClearValue::Float([0.0, 0.0, 0.0, 1.0])),
                        ..RenderingAttachmentInfo::image_view(res.swapchain_image_views[image_index as usize].clone())
                    })],
                    ..Default::default()
                })?
                .bind_pipeline_graphics(res.present_pipeline.clone())?
                .set_viewport(0, smallvec![viewport])?
                .set_scissor(0, smallvec![scissor])?
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    res.present_pipeline_layout.clone(),
                    0,
                    res.present_descriptor_set.clone(),
                )?
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    res.present_pipeline_layout.clone(),
                    1,
                    res.slides_descriptor_set.clone(),
                )?
                .push_constants(res.present_pipeline_layout.clone(), 0, pc)?;
            unsafe { builder.draw(3, 1, 0, 0)?; }
            builder.end_rendering()?;
        } else {
            // Direct rendering: Instant, Slide, or non-transitioning
            builder
                .begin_rendering(RenderingInfo {
                    color_attachments: vec![Some(RenderingAttachmentInfo {
                        load_op: AttachmentLoadOp::Clear,
                        store_op: AttachmentStoreOp::Store,
                        clear_value: Some(ClearValue::Float([0.0, 0.0, 0.0, 1.0])),
                        ..RenderingAttachmentInfo::image_view(res.swapchain_image_views[image_index as usize].clone())
                    })],
                    ..Default::default()
                })?
                .bind_pipeline_graphics(res.direct_pipeline.clone())?
                .set_viewport(0, smallvec![viewport])?
                .set_scissor(0, smallvec![scissor])?
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    res.direct_pipeline_layout.clone(),
                    0,
                    res.slides_descriptor_set.clone(),
                )?
                .push_constants(res.direct_pipeline_layout.clone(), 0, pc)?;
            unsafe { builder.draw(3, 1, 0, 0)?; }
            builder.end_rendering()?;
        }

        let command_buffer = builder.build()?;

        let future: Box<dyn GpuFuture> = Box::new(
            acquire_future
                .then_execute(res.queue.clone(), command_buffer)?
                .then_swapchain_present(
                    res.queue.clone(),
                    SwapchainPresentInfo::swapchain_image_index(
                        res.swapchain.clone(),
                        image_index,
                    ),
                )
                .then_signal_fence_and_flush()?,
        );

        self.previous_frame = Some(future);

        Ok(())
    }
}

