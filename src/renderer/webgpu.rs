mod wgpu_vec;
use wgpu::util::RenderEncoder;
pub use wgpu_vec::*;

mod wgpu_queue;
pub use wgpu_queue::*;

mod wgpu_texture;
pub use wgpu_texture::*;

mod wgpu_stencil_texture;
pub use wgpu_stencil_texture::*;

mod wgpu_ext;
pub use wgpu_ext::*;

mod wgpu_pipeline_cache;
pub use wgpu_pipeline_cache::*;

mod mem_align;
pub use mem_align::*;

use crate::{
    renderer::{
        ImageId,
        Vertex,
    },
    BlendFactor,
    Color,
    CompositeOperationState,
    ErrorKind,
    FillRule,
    ImageInfo,
    ImageSource,
    ImageStore,
    Rect,
    Size,
};

use super::{
    Command,
    CommandType,
    Params,
    RenderTarget,
    Renderer,
};

// use fnv::FnvHashMap;
use imgref::ImgVec;
use rgb::RGBA8;
use std::borrow::Cow;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct WGPUBlend {
    pub src_rgb: wgpu::BlendFactor,
    pub dst_rgb: wgpu::BlendFactor,
    pub src_alpha: wgpu::BlendFactor,
    pub dst_alpha: wgpu::BlendFactor,
}

impl From<BlendFactor> for wgpu::BlendFactor {
    fn from(a: BlendFactor) -> Self {
        match a {
            BlendFactor::Zero => Self::Zero,
            BlendFactor::One => Self::One,
            BlendFactor::SrcColor => Self::SrcColor,
            BlendFactor::OneMinusSrcColor => Self::OneMinusSrcColor,
            BlendFactor::DstColor => Self::DstColor,
            BlendFactor::OneMinusDstColor => Self::OneMinusDstColor,
            BlendFactor::SrcAlpha => Self::SrcAlpha,
            BlendFactor::OneMinusSrcAlpha => Self::OneMinusSrcAlpha,
            BlendFactor::DstAlpha => Self::DstAlpha,
            BlendFactor::OneMinusDstAlpha => Self::OneMinusDstAlpha,
            BlendFactor::SrcAlphaSaturate => Self::SrcAlphaSaturated,
        }
    }
}

impl From<CompositeOperationState> for WGPUBlend {
    fn from(v: CompositeOperationState) -> Self {
        Self {
            src_rgb: v.src_rgb.into(),
            dst_rgb: v.dst_rgb.into(),
            src_alpha: v.src_alpha.into(),
            dst_alpha: v.dst_alpha.into(),
        }
    }
}

pub struct WGPUStates {}

impl WGPUStates {
    pub fn new() -> Self {
        Self {}
    }
}

/// the things that
pub struct WGPU {
    default_stencil_state: wgpu::RenderPipeline,
    fill_shape_stencil_state: wgpu::RenderPipeline,
    fill_anti_alias_stencil_state_nonzero: wgpu::RenderPipeline,
    fill_anti_alias_stencil_state_evenodd: wgpu::RenderPipeline,
    fill_stencil_state_nonzero: wgpu::RenderPipeline,
    fill_stencil_state_evenodd: wgpu::RenderPipeline,

    stroke_shape_stencil_state: wgpu::RenderPipeline,
    stroke_anti_alias_stencil_state: wgpu::RenderPipeline,
    stroke_clear_stencil_state: wgpu::RenderPipeline,

    convex_fill1: wgpu::RenderPipeline,
    convex_fill2: wgpu::RenderPipeline,

    stencil_texture: WGPUStencilTexture,
    index_buffer: WGPUVec<u32>,
    vertex_buffer: WGPUVec<Vertex>,
    render_target: RenderTarget,
    pseudo_texture: WGPUTexture,

    view_size: Size,
}

impl WGPU {
    pub fn new(device: &wgpu::Device) -> Self {
        // let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
        //     label: None,
        //     source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("webgpu/shader.wgsl"))),
        //     flags: wgpu::ShaderFlags::all(),
        // });

        let default_stencil_state = 0;

        let clear_stencil_state = {
            let front = wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Always,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            };

            let state = wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState {
                    front,
                    //todo: is default the as None?
                    back: Default::default(),
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: Default::default(),
                clamp_depth: false,
            };
        };

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: std::num::NonZeroU32::new(2),
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureViewArray(&[]),
            }],
        });

        // bind_group.destroy();

        let fill_shape_stencil_state = 0;
        let fill_anti_alias_stencil_state_nonzero = 0;
        let fill_anti_alias_stencil_state_evenodd = 0;
        let fill_stencil_state_nonzero = 0;
        let fill_stencil_state_evenodd = 0;
        let stroke_shape_stencil_state = 0;
        let stroke_anti_alias_stencil_state = 0;
        let stroke_clear_stencil_state = 0;

        todo!()
        // Self {

        // }
    }

    fn convex_fill<'a>(
        &'a mut self,
        pass: &mut wgpu::RenderPass<'a>,
        images: &ImageStore<WGPUTexture>,
        cmd: &Command,
        paint: Params,
    ) {
        // encoder.push_debug_group("convex_fill");

        for drawable in &cmd.drawables {
            if let Some((start, count)) = drawable.fill_verts {
                //
                pass.set_pipeline(&self.convex_fill1);

                let offset = self.index_buffer.len();
                let triangle_fan_index_count = self
                    .index_buffer
                    .extend_with_triange_fan_indices_cw(start as u32, count as u32);

                // encoder.begin_render_pass(desc)
                // render_pass.draw_indexed(indices, base_vertex, instances)
                // pass.set_index_buffer(buffer_slice, );
                let fmt = wgpu::IndexFormat::Uint32;
                // pass.set_index_buffer(self.index_buffer, fmt);
                pass.draw_indexed(0..0, 0, 0..0);
            }

            if let Some((start, count)) = drawable.stroke_verts {
                pass.set_pipeline(&self.convex_fill2);
                let vertex_range = start as _..(start + count) as _;
                pass.draw(vertex_range, 0..0);
            }
        }
    }

    fn concave_fill<'a>(
        &'a mut self,
        pass: &mut wgpu::RenderPass<'a>,
        images: &ImageStore<WGPUTexture>,
        cmd: &Command,
        stencil_paint: Params,
        fill_paint: Params,
    ) {
    }

    fn stroke<'a>(
        &'a mut self,
        pass: &mut wgpu::RenderPass<'a>,
        images: &ImageStore<WGPUTexture>,
        cmd: &Command,
        paint: Params,
    ) {
        //
        // draws triangle strip
        self.set_uniforms(pass, images, paint, cmd.image, cmd.alpha_mask);
        for drawable in &cmd.drawables {
            if let Some((start, count)) = drawable.stroke_verts {
                // pass.draw()
            }
        }
    }

    fn stencil_stroke<'a>(
        &'a mut self,
        pass: &mut wgpu::RenderPass<'a>,
        images: &ImageStore<WGPUTexture>,
        cmd: &Command,
        paint1: Params,
        paint2: Params,
    ) {
        //
        // pass.set_pipeline(pipeline);
        // self.set_uniforms(pass, images, image_tex, alpha_tex)
    }

    fn triangles<'a>(
        &'a mut self,
        pass: &mut wgpu::RenderPass<'a>,
        images: &ImageStore<WGPUTexture>,
        cmd: &Command,
        paint: Params,
    ) {
        //
        self.set_uniforms(pass, images, paint, cmd.image, cmd.alpha_mask);
        // pass.set_pipeline(pipeline)
        if let Some((start, count)) = cmd.triangles_verts {
            // pass.draw(vertices, instances)
        }
    }

    fn set_uniforms<'a>(
        &self,
        pass: &wgpu::RenderPass<'a>,
        images: &ImageStore<WGPUTexture>,
        paint: Params,
        image_tex: Option<ImageId>,
        alpha_tex: Option<ImageId>,
    ) {
        let tex = if let Some(id) = image_tex {
            images.get(id).unwrap()
        } else {
            &self.pseudo_texture
        };
        // pass.set_viewport(x, y, w, h, min_depth, max_depth)
    }

    fn clear_rect<'a>(
        &'a mut self,
        pass: &mut wgpu::RenderPass<'a>,
        images: &ImageStore<WGPUTexture>,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: Color,
    ) {
        //
        let ndc_rect = Rect {
            x: -1.0,
            y: -1.0,
            w: 2.0,
            h: 2.0,
        };
    }

    pub fn set_target(&mut self, images: &ImageStore<WGPUTexture>, target: RenderTarget) {
        //
        if self.render_target == target {}

        let size = match target {
            RenderTarget::Screen => todo!(),
            RenderTarget::Image(id) => {
                let texture = images.get(id).unwrap();
                texture.size()
            }
        };
        self.render_target = target;
        self.view_size = size;
    }
}

impl Renderer for WGPU {
    type Image = WGPUTexture;
    fn set_size(&mut self, width: u32, height: u32, dpi: f32) {
        let size = Size::new(width as f32, height as f32);
        self.view_size = size;
    }

    fn render(&mut self, images: &ImageStore<Self::Image>, verts: &[Vertex], commands: &[Command]) {
        todo!()
    }

    fn alloc_image(&mut self, info: ImageInfo) -> Result<Self::Image, ErrorKind> {
        todo!()
    }

    fn update_image(
        &mut self,
        image: &mut Self::Image,
        data: ImageSource,
        x: usize,
        y: usize,
    ) -> Result<(), ErrorKind> {
        todo!()
    }

    fn delete_image(&mut self, image: Self::Image) {
        image.delete();
    }

    fn screenshot(&mut self) -> Result<ImgVec<RGBA8>, ErrorKind> {
        todo!()
    }
}

impl From<Color> for wgpu::Color {
    fn from(a: Color) -> Self {
        todo!()
    }
}

pub struct RenderPass<'a> {
    inner: wgpu::RenderPass<'a>,
}

impl<'a> RenderPass<'a> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn set_viewport(&self) {
        // self.inner.set_viewport(x, y, w, h, min_depth, max_depth)
    }

    pub fn set_fragment(&self) {
        todo!()
        // self.inner.set_push_constants(stages, offset, data)
    }
}

fn new_render_command_encoder<'a>(
    ctx: WGPUContext,
    target: &wgpu::TextureView,
    command_buffer: &'a wgpu::CommandBuffer,
    clear_color: Color,
    stencil_texture: &mut WGPUStencilTexture,
    vertex_buffer: &WGPUVec<Vertex>,
) -> wgpu::CommandEncoder {
    let desc = wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
            attachment: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear_color.into()),
                store: false,
            },
        }],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
            attachment: stencil_texture.tex(), //&'a TextureView,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: false,
            }), //Option<Operations<f32>>,
            stencil_ops: None,                 //Option<Operations<u32>>,
        }),
    };

    // todo set cull mode on the

    let encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // encoder.set_vertex_buffer(0, vertex_buffer.as_slice());
    // encoder

    encoder
}
