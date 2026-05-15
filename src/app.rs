use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use half::f16;
use smallvec::smallvec;
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, PrimaryCommandBufferAbstract,
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
use vulkano::pipeline::graphics::color_blend::{AttachmentBlend, BlendFactor, BlendOp, ColorBlendAttachmentState, ColorBlendState, ColorComponents};
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

mod fs_blend {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"
#version 460
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;

layout(push_constant) uniform PC {
    int   current_layer;
    int   previous_layer;
    float new_alpha;
    float blur_radius;
    float slide_offset_x;
    float slide_offset_y;
} pc;

layout(set = 0, binding = 0) uniform sampler2DArray slides;

void main() {
    vec3 slide = texture(slides, vec3(v_uv, pc.current_layer)).rgb;
    f_color = vec4(slide, pc.new_alpha);
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
    float new_alpha;
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
    float new_alpha;
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
    float new_alpha;
    float blur_radius;
    float slide_offset_x;
    float slide_offset_y;
} pc;

layout(set = 0, binding = 0) uniform sampler2D feedback;
layout(set = 0, binding = 1) uniform sampler2DArray slides;

void main() {
    vec3 fb = texture(feedback, v_uv).rgb;
    vec3 slide = texture(slides, vec3(v_uv, pc.current_layer)).rgb;
    vec3 result = mix(fb, slide, pc.new_alpha);
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
    float new_alpha;
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
}

#[derive(Clone)]
pub struct AppConfig {
    pub slides_path: std::path::PathBuf,
    pub transition_type: TransitionType,
    pub blur_radius_max: f32,
    pub transition_duration: f32,
    pub profiling: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            slides_path: std::path::PathBuf::new(),
            transition_type: TransitionType::Smooth,
            blur_radius_max: 20.0,
            transition_duration: 0.5,
            profiling: false,
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
  --transition-type <type>     Transition style: smooth (default), instant, or slide
  --blur-radius <px>           Max Gaussian blur radius (smooth only; default: 20.0)
  --transition-duration <sec>  Transition duration in seconds (slide; default: 0.5)
  --profile                    Print per-frame timing breakdown every second
  --help                       Show this help

Transition types:
  smooth  - Compute-shader blur with feedback ping-pong (default)
  instant - No animation, immediate cut
  slide   - New slide slides in; from bottom for slides, from right for chapters"
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
                    _ => {
                        eprintln!("Error: --transition-type must be 'smooth', 'instant', or 'slide'");
                        return None;
                    }
                };
            }
            "--blur-radius" => {
                i += 1;
                config.blur_radius_max = args.get(i)?.parse().ok()?;
            }
            "--transition-duration" => {
                i += 1;
                config.transition_duration = args.get(i)?.parse().ok()?;
            }
                "--profile" => {
                config.profiling = true;
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
                    notes: "Customize the viewing experience:\n\n- `--blur-radius`: Max Gaussian blur during transitions\n- `--transition-duration`: Transition timing\n\nDefault values work well for most presentations.",
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
    pub _device: Arc<Device>,
    pub queue: Arc<vulkano::device::Queue>,
    pub _memory_allocator: Arc<StandardMemoryAllocator>,
    pub descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    pub command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    pub swapchain: Arc<Swapchain>,
    pub _swapchain_images: Vec<Arc<Image>>,
    pub swapchain_image_views: Vec<Arc<ImageView>>,

    // Direct rendering pipeline (for Instant/Slide and non-transitioning)
    pub direct_pipeline: Arc<vulkano::pipeline::graphics::GraphicsPipeline>,
    pub direct_pipeline_layout: Arc<PipelineLayout>,

    // Smooth transition feedback pipelines
    pub blend_pipeline: Arc<vulkano::pipeline::graphics::GraphicsPipeline>,
    pub blend_pipeline_layout: Arc<PipelineLayout>,
    pub blur_h_pipeline: Arc<ComputePipeline>,
    pub blur_v_pipeline: Arc<ComputePipeline>,
    pub blur_pipeline_layout: Arc<PipelineLayout>,
    pub present_pipeline: Arc<vulkano::pipeline::graphics::GraphicsPipeline>,
    pub present_pipeline_layout: Arc<PipelineLayout>,

    // Slides sampler descriptor set (shared by direct and blend pipelines)
    pub slides_descriptor_set: Arc<DescriptorSet>,

    // Feedback textures
    pub feedback: [Arc<Image>; 2],
    pub feedback_view: [Arc<ImageView>; 2],
    pub ping_image: Arc<Image>,
    pub ping_view: Arc<ImageView>,
    pub sampler: Arc<Sampler>,

    // Present descriptor sets (one per feedback texture)
    pub present_descriptor_set: [Arc<DescriptorSet>; 2],

    // Blur descriptor sets
    pub blur_h_descriptor_set: [Arc<DescriptorSet>; 2], // [src=fb[0]→ping, src=fb[1]→ping]
    pub blur_v_descriptor_set: [Arc<DescriptorSet>; 2], // [src=ping→fb[0], src=ping→fb[1]]

    pub _format: Format,
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
    feedback_idx: usize,
    frame_count: u64,
    last_fps_print: Instant,
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
            usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::SAMPLED | ImageUsage::STORAGE,
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

    let (_texture_image, texture_view, sampler) = create_texture_array(
        device.clone(),
        memory_allocator.clone(),
        queue.clone(),
        cmd_allocator.clone(),
        &slide_paths,
    )
    .map_err(|e| format!("create_texture_array: {e}"))?;

    let scene_extent = swapchain_images[0].extent();
    let extent2 = [scene_extent[0], scene_extent[1]];

    // Create feedback textures (ping-pong pair) and intermediate ping buffer
    let feedback_img_0 = create_feedback_image(&memory_allocator, extent2)?;
    let feedback_img_1 = create_feedback_image(&memory_allocator, extent2)?;
    let feedback_view_0 = create_feedback_view(feedback_img_0.clone())?;
    let feedback_view_1 = create_feedback_view(feedback_img_1.clone())?;

    let ping_image = Image::new(
        memory_allocator.clone() as Arc<dyn vulkano::memory::allocator::MemoryAllocator>,
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R16G16B16A16_SFLOAT,
            extent: [scene_extent[0], scene_extent[1], 1],
            array_layers: 1,
            mip_levels: 1,
            usage: ImageUsage::STORAGE | ImageUsage::SAMPLED,
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
                size: 24,
            }],
            ..Default::default()
        },
    )?;

    // Blend pipeline: renders slide onto feedback with alpha blending
    let blend_pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineLayoutCreateInfo {
            set_layouts: vec![slides_descriptor_set_layout.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: vulkano::shader::ShaderStages::FRAGMENT,
                offset: 0,
                size: 24,
            }],
            ..Default::default()
        },
    )?;

    let vs_module = vs::load(device.clone())
        .map_err(|e| format!("vs load: {e}"))?;
    let fs_blend_module = fs_blend::load(device.clone())
        .map_err(|e| format!("fs_blend load: {e}"))?;
    let cs_blur_module = cs_blur::load(device.clone())
        .map_err(|e| format!("cs_blur load: {e}"))?;
    let cs_blur_v_module = cs_blur_v::load(device.clone())
        .map_err(|e| format!("cs_blur_v load: {e}"))?;
    let fs_present_module = fs_present::load(device.clone())
        .map_err(|e| format!("fs_present load: {e}"))?;
    let fs_direct_module = fs_direct::load(device.clone())
        .map_err(|e| format!("fs_direct load: {e}"))?;

    let vs_entry = vs_module
        .entry_point("main")
        .ok_or("Vertex shader entry point 'main' not found")?;
    let fs_blend_entry = fs_blend_module
        .entry_point("main")
        .ok_or("Fs_blend fragment shader entry point 'main' not found")?;
    let cs_blur_entry = cs_blur_module
        .entry_point("main")
        .ok_or("Cs_blur compute shader entry point 'main' not found")?;
    let cs_blur_v_entry = cs_blur_v_module
        .entry_point("main")
        .ok_or("Cs_blur_v compute shader entry point 'main' not found")?;
    let fs_present_entry = fs_present_module
        .entry_point("main")
        .ok_or("Fs_present fragment shader entry point 'main' not found")?;
    let fs_direct_entry = fs_direct_module
        .entry_point("main")
        .ok_or("Fs_direct fragment shader entry point 'main' not found")?;

    // Blend pipeline (smooth, blends slide onto feedback)
    let blend_pipeline =
        vulkano::pipeline::graphics::GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: smallvec![
                    PipelineShaderStageCreateInfo::new(vs_entry.clone()),
                    PipelineShaderStageCreateInfo::new(fs_blend_entry),
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
                    ColorBlendAttachmentState {
                        blend: Some(AttachmentBlend {
                            src_color_blend_factor: BlendFactor::SrcAlpha,
                            dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                            color_blend_op: BlendOp::Add,
                            src_alpha_blend_factor: BlendFactor::One,
                            dst_alpha_blend_factor: BlendFactor::Zero,
                            alpha_blend_op: BlendOp::Add,
                        }),
                        color_write_mask: ColorComponents::all(),
                        ..Default::default()
                    },
                )),
                subpass: Some(PipelineSubpassType::BeginRendering(
                    PipelineRenderingCreateInfo {
                        color_attachment_formats: vec![Some(Format::R16G16B16A16_SFLOAT)],
                        ..Default::default()
                    },
                )),
                ..GraphicsPipelineCreateInfo::layout(blend_pipeline_layout.clone())
            },
        )
        .map_err(|e| format!("blend_pipeline: {e}"))?;

    // Present pipeline (smooth, draws feedback + slide to swapchain)
    let present_descriptor_set_layout = DescriptorSetLayout::new(
        device.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: std::collections::BTreeMap::from([
                (0u32, DescriptorSetLayoutBinding {
                    descriptor_count: 1,
                    stages: vulkano::shader::ShaderStages::FRAGMENT,
                    ..DescriptorSetLayoutBinding::descriptor_type(DescriptorType::CombinedImageSampler)
                }),
                (1u32, DescriptorSetLayoutBinding {
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
            set_layouts: vec![present_descriptor_set_layout.clone()],
            push_constant_ranges: vec![PushConstantRange {
                stages: vulkano::shader::ShaderStages::FRAGMENT,
                offset: 0,
                size: 24,
            }],
            ..Default::default()
        },
    )?;

    // Create present descriptor sets (one per feedback texture)
    let make_present_set = |feedback_view: Arc<ImageView>| -> Result<Arc<DescriptorSet>, Box<dyn std::error::Error>> {
        DescriptorSet::new(
            descriptor_set_allocator.clone(),
            present_descriptor_set_layout.clone(),
            [
                WriteDescriptorSet::image_view_with_layout_sampler(
                    0,
                    DescriptorImageViewInfo {
                        image_view: feedback_view,
                        image_layout: ImageLayout::General,
                    },
                    sampler.clone(),
                ),
                WriteDescriptorSet::image_view_sampler(
                    1,
                    texture_view.clone(),
                    sampler.clone(),
                ),
            ],
            [],
        )
        .map_err(|e| format!("present descriptor_set: {e}").into())
    };

    let present_set_0 = make_present_set(feedback_view_0.clone())?;
    let present_set_1 = make_present_set(feedback_view_1.clone())?;

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
                size: 24,
            }],
            ..Default::default()
        },
    )?;

    // Create blur descriptor sets for both permutations
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

    let blur_h_set_0 = make_blur_set(feedback_view_0.clone(), ping_view.clone())?;
    let blur_h_set_1 = make_blur_set(feedback_view_1.clone(), ping_view.clone())?;
    let blur_v_set_0 = make_blur_set(ping_view.clone(), feedback_view_0.clone())?;
    let blur_v_set_1 = make_blur_set(ping_view.clone(), feedback_view_1.clone())?;

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

    if let Some(meta) = collection.meta(0) {
        println!("{}", texture::format_slide_display(meta, 0, collection.len()));
    }

    Ok(App {
        resources: GpuResources {
            _device: device,
            queue,
            _memory_allocator: memory_allocator,
            descriptor_set_allocator,
            command_buffer_allocator: cmd_allocator,
            swapchain,
            _swapchain_images: swapchain_images,
            swapchain_image_views,
            direct_pipeline,
            direct_pipeline_layout,
            blend_pipeline,
            blend_pipeline_layout,
            blur_h_pipeline,
            blur_v_pipeline,
            blur_pipeline_layout,
            present_pipeline,
            present_pipeline_layout,
            slides_descriptor_set,
            feedback: [feedback_img_0, feedback_img_1],
            feedback_view: [feedback_view_0, feedback_view_1],
            ping_image,
            ping_view,
            sampler,
            present_descriptor_set: [present_set_0, present_set_1],
            blur_h_descriptor_set: [blur_h_set_0, blur_h_set_1],
            blur_v_descriptor_set: [blur_v_set_0, blur_v_set_1],
            _format: format,
            window,
        },
        collection,
        current_layer: 0,
        target_layer: 0,
        previous_layer: 0,
        transition_time: 0.0,
        is_transitioning: false,
        transition_direction: (0.0, 0.0),
        feedback_idx: 0,
        frame_count: 0,
        last_fps_print: Instant::now(),
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
    new_alpha: f32,
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
            if self.config.profiling {
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
        }
    }

    pub fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        drop(self.previous_frame.take());

        let _t0 = Instant::now();
        let res = &mut self.resources;

        let (image_index, _is_suboptimal, acquire_future) =
            vulkano::swapchain::acquire_next_image(res.swapchain.clone(), None)?;
        let _t1 = Instant::now();

        let t = self.transition_time;
        let new_alpha = if self.is_transitioning {
            let u = (t / self.config.transition_duration).min(1.0);
            u * u * (3.0 - 2.0 * u)
        } else {
            1.0
        };
        let blur_radius = if self.is_transitioning && self.config.transition_type == TransitionType::Smooth {
            self.config.blur_radius_max
        } else {
            0.0
        };

        let (slide_offset_x, slide_offset_y) = if self.is_transitioning && self.config.transition_type == TransitionType::Slide {
            let u = (t / self.config.transition_duration).min(1.0);
            let ease_out = 1.0 - (1.0 - u) * (1.0 - u) * (1.0 - u);
            (
                self.transition_direction.0 * (1.0 - ease_out),
                self.transition_direction.1 * (1.0 - ease_out),
            )
        } else {
            (0.0, 0.0)
        };

        let pc = PushConstants {
            current_layer: self.current_layer as i32,
            previous_layer: self.previous_layer as i32,
            new_alpha,
            blur_radius,
            slide_offset_x,
            slide_offset_y,
        };

        let extent = res._swapchain_images[image_index as usize].extent();
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

        if self.config.transition_type == TransitionType::Smooth && self.is_transitioning {
            let idx = self.feedback_idx;
            let out_idx = 1 - idx;

            // Pass 1: Blend current slide onto input feedback
            builder
                .begin_rendering(RenderingInfo {
                    color_attachments: vec![Some(RenderingAttachmentInfo {
                        load_op: AttachmentLoadOp::Load,
                        store_op: AttachmentStoreOp::Store,
                        clear_value: None,
                        image_layout: ImageLayout::General,
                        ..RenderingAttachmentInfo::image_view(res.feedback_view[idx].clone())
                    })],
                    ..Default::default()
                })?
                .bind_pipeline_graphics(res.blend_pipeline.clone())?
                .set_viewport(0, smallvec![viewport.clone()])?
                .set_scissor(0, smallvec![scissor.clone()])?
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    res.blend_pipeline_layout.clone(),
                    0,
                    res.slides_descriptor_set.clone(),
                )?
                .push_constants(res.blend_pipeline_layout.clone(), 0, pc)?;
            unsafe { builder.draw(3, 1, 0, 0)?; }
            builder.end_rendering()?;

            // Pass 2: Horizontal blur (feedback → ping)
            builder
                .bind_pipeline_compute(res.blur_h_pipeline.clone())?
                .bind_descriptor_sets(
                    PipelineBindPoint::Compute,
                    res.blur_pipeline_layout.clone(),
                    0,
                    res.blur_h_descriptor_set[idx].clone(),
                )?
                .push_constants(res.blur_pipeline_layout.clone(), 0, pc)?;
            unsafe {
                builder.dispatch([
                    (extent[0] + 15) / 16,
                    (extent[1] + 15) / 16,
                    1,
                ])?;
            }

            // Pass 3: Vertical blur (ping → output feedback)
            builder
                .bind_pipeline_compute(res.blur_v_pipeline.clone())?
                .bind_descriptor_sets(
                    PipelineBindPoint::Compute,
                    res.blur_pipeline_layout.clone(),
                    0,
                    res.blur_v_descriptor_set[idx].clone(),
                )?
                .push_constants(res.blur_pipeline_layout.clone(), 0, pc)?;
            unsafe {
                builder.dispatch([
                    (extent[0] + 15) / 16,
                    (extent[1] + 15) / 16,
                    1,
                ])?;
            }

            // Pass 4: Present output feedback + current slide to swapchain
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
                    res.present_descriptor_set[idx].clone(),
                )?
                .push_constants(res.present_pipeline_layout.clone(), 0, pc)?;
            unsafe { builder.draw(3, 1, 0, 0)?; }
            builder.end_rendering()?;

            // Swap feedback buffers for next frame
            self.feedback_idx = out_idx;
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
        let _t2 = Instant::now();

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
        let _t3 = Instant::now();

        self.previous_frame = Some(future);
        let _t4 = Instant::now();

        if self.config.profiling {
            self.frame_count += 1;
            let now = Instant::now();
            let since_last_log = now.duration_since(self.last_fps_print);
            if since_last_log.as_secs_f32() >= 1.0 {
                println!(
                    "FPS: {:.1} | acquire: {:.3}ms | build_cb: {:.3}ms | submit: {:.3}ms | store_prev: {:.3}ms | total: {:.3}ms",
                    self.frame_count as f32 / since_last_log.as_secs_f32().max(0.001),
                    (_t1 - _t0).as_secs_f32() * 1000.0,
                    (_t2 - _t1).as_secs_f32() * 1000.0,
                    (_t3 - _t2).as_secs_f32() * 1000.0,
                    (_t4 - _t3).as_secs_f32() * 1000.0,
                    (_t4 - _t0).as_secs_f32() * 1000.0,
                );
                self.frame_count = 0;
                self.last_fps_print = now;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compute_new_alpha(t: f32, fade_dur: f32) -> f32 {
        let u = (t / fade_dur).min(1.0);
        u * u * (3.0 - 2.0 * u)
    }

    fn compute_blur(t: f32, blur_dur: f32) -> f32 {
        if t >= blur_dur { 0.0 } else { 20.0 }
    }

    #[test]
    fn transition_params_t0() {
        assert_eq!(compute_new_alpha(0.0, 0.5), 0.0);
        assert_eq!(compute_blur(0.0, 0.5), 20.0);
    }

    #[test]
    fn transition_params_t0_25() {
        let a = compute_new_alpha(0.25, 0.5);
        assert!(a > 0.0 && a < 1.0);
    }

    #[test]
    fn transition_params_t0_5() {
        assert!((compute_new_alpha(0.5, 0.5) - 1.0).abs() < 0.001);
    }

    #[test]
    fn transition_params_t10() {
        assert!((compute_new_alpha(10.0, 0.5) - 1.0).abs() < 0.001);
        // Blur stops when not transitioning
        assert!((compute_blur(10.0, 0.5) - 0.0).abs() < 0.001);
    }

    #[test]
    fn transition_smoothstep_shape() {
        let a1 = compute_new_alpha(0.125, 0.5);
        let a2 = compute_new_alpha(0.25, 0.5);
        let a3 = compute_new_alpha(0.375, 0.5);
        assert!((a2 - a1 - (a3 - a2)).abs() < 0.001);
    }

    #[test]
    fn instant_transition_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.transition_type, TransitionType::Smooth);
    }

    #[test]
    fn parse_args_transition_type_instant() {
        let args = vec![
            "rs-vulkan".to_string(),
            "/path/to/slides".to_string(),
            "--transition-type".to_string(),
            "instant".to_string(),
        ];
        let config = parse_args(&args).unwrap();
        assert_eq!(config.transition_type, TransitionType::Instant);
    }

    #[test]
    fn parse_args_transition_type_invalid() {
        let args = vec![
            "rs-vulkan".to_string(),
            "/path/to/slides".to_string(),
            "--transition-type".to_string(),
            "foo".to_string(),
        ];
        assert!(parse_args(&args).is_none());
    }

    fn compute_slide_offset(t: f32, dur: f32, direction: (f32, f32)) -> (f32, f32) {
        let u = (t / dur).min(1.0);
        let ease_out = 1.0 - (1.0 - u) * (1.0 - u) * (1.0 - u);
        (direction.0 * (1.0 - ease_out), direction.1 * (1.0 - ease_out))
    }

    #[test]
    fn slide_offset_starts_at_direction() {
        let (ox, oy) = compute_slide_offset(0.0, 10.0, (0.0, 1.0));
        assert!((ox - 0.0).abs() < 0.001);
        assert!((oy - 1.0).abs() < 0.001);
    }

    #[test]
    fn slide_offset_ends_at_zero() {
        let (ox, oy) = compute_slide_offset(10.0, 10.0, (0.0, 1.0));
        assert!((ox - 0.0).abs() < 0.001);
        assert!((oy - 0.0).abs() < 0.001);
    }

    #[test]
    fn slide_offset_chapter_direction() {
        let (ox, oy) = compute_slide_offset(0.0, 10.0, (1.0, 0.0));
        assert!((ox - 1.0).abs() < 0.001);
        assert!((oy - 0.0).abs() < 0.001);
    }

    #[test]
    fn slide_offset_prev_direction() {
        let (ox, oy) = compute_slide_offset(0.0, 10.0, (0.0, -1.0));
        assert!((ox - 0.0).abs() < 0.001);
        assert!((oy - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn slide_offset_prev_chapter_direction() {
        let (ox, oy) = compute_slide_offset(0.0, 10.0, (-1.0, 0.0));
        assert!((ox - (-1.0)).abs() < 0.001);
        assert!((oy - 0.0).abs() < 0.001);
    }

    #[test]
    fn slide_ease_out_midway() {
        let u = 5.0 / 10.0;
        let ease_out = 1.0 - (1.0 - u) * (1.0 - u) * (1.0 - u);
        let (ox, oy) = compute_slide_offset(5.0, 10.0, (0.0, 1.0));
        assert!((ox - 0.0).abs() < 0.001);
        assert!((oy - (1.0 - ease_out)).abs() < 0.001);
    }

    #[test]
    fn transition_type_eq_ordering() {
        assert_eq!(TransitionType::Smooth, TransitionType::Smooth);
        assert_eq!(TransitionType::Instant, TransitionType::Instant);
        assert_eq!(TransitionType::Slide, TransitionType::Slide);
        assert_ne!(TransitionType::Smooth, TransitionType::Instant);
    }

    #[test]
    fn parse_args_default_config() {
        let config = parse_args(&["program".into(), "/slides".into()]);
        assert!(config.is_some());
        let cfg = config.unwrap();
        assert_eq!(cfg.slides_path, std::path::PathBuf::from("/slides"));
        assert!((cfg.blur_radius_max - 20.0).abs() < 1e-6);
        assert!((cfg.transition_duration - 0.5).abs() < 1e-6);
    }

    #[test]
    fn parse_args_custom_values() {
        let args: Vec<String> = vec![
            "program".into(), "/slides".into(),
            "--blur-radius".into(), "30.0".into(),
            "--transition-duration".into(), "1.0".into(),
        ];
        let config = parse_args(&args);
        assert!(config.is_some());
        let cfg = config.unwrap();
        assert!((cfg.blur_radius_max - 30.0).abs() < 1e-6);
        assert!((cfg.transition_duration - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_args_transition_type_instant2() {
        let config = parse_args(&[
            "program".into(), "/slides".into(),
            "--transition-type".into(), "instant".into(),
        ]);
        assert!(config.is_some());
        assert_eq!(config.unwrap().transition_type, TransitionType::Instant);
    }

    #[test]
    fn parse_args_transition_type_smooth_explicit() {
        let config = parse_args(&[
            "program".into(), "/slides".into(),
            "--transition-type".into(), "smooth".into(),
        ]);
        assert!(config.is_some());
        assert_eq!(config.unwrap().transition_type, TransitionType::Smooth);
    }

    #[test]
    fn parse_args_transition_type_default() {
        let config = parse_args(&["program".into(), "/slides".into()]);
        assert!(config.is_some());
        assert_eq!(config.unwrap().transition_type, TransitionType::Smooth);
    }

    #[test]
    fn parse_args_transition_type_slide() {
        let config = parse_args(&[
            "program".into(), "/slides".into(),
            "--transition-type".into(), "slide".into(),
        ]);
        assert!(config.is_some());
        assert_eq!(config.unwrap().transition_type, TransitionType::Slide);
    }

    #[test]
    fn parse_args_transition_type_invalid2() {
        assert!(parse_args(&[
            "program".into(), "/slides".into(),
            "--transition-type".into(), "bogus".into(),
        ]).is_none());
    }

    #[test]
    fn parse_args_help_returns_none() {
        assert!(parse_args(&["program".into(), "--help".into()]).is_none());
    }

    #[test]
    fn parse_args_no_args_returns_none() {
        assert!(parse_args(&["program".into()]).is_none());
    }

    #[test]
    fn parse_args_unknown_option_returns_none() {
        assert!(parse_args(&["program".into(), "/slides".into(), "--bogus".into()]).is_none());
    }

    #[test]
    fn parse_args_invalid_number_returns_none() {
        assert!(parse_args(&["program".into(), "/slides".into(), "--blur-radius".into(), "abc".into()]).is_none());
    }

    #[test]
    fn parse_args_init_returns_none() {
        let dir = tempfile::TempDir::with_prefix("init_test").unwrap();
        let path = dir.path().join("example");
        let result = parse_args(&["program".into(), "init".into(), path.to_string_lossy().into()]);
        assert!(result.is_none());
        assert!(path.join("presenter_notes.md").exists());
    }

    #[test]
    fn parse_args_init_missing_path_returns_none() {
        let result = parse_args(&["program".into(), "init".into()]);
        assert!(result.is_none());
    }
}
