
use image::DynamicImage;

use crate::{ImageFlags, Vertex, Paint, Scissor, Path, Color, LineJoin, Transform2D};
use crate::path::{Convexity, CachedPath};
use crate::renderer::{TextureType, ImageId, Renderer};

mod opengl;
pub use opengl::OpenGl;

pub trait GpuRendererBackend {
    fn clear_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color);
    fn set_size(&mut self, width: u32, height: u32, dpi: f32);

    fn render(&mut self, verts: &[Vertex], commands: &[Command]);

    // TODO: rethink this texture API
    fn create_texture(&mut self, texture_type: TextureType, width: u32, height: u32, flags: ImageFlags) -> ImageId;
    fn update_texture(&mut self, id: ImageId, image: &DynamicImage, x: u32, y: u32, w: u32, h: u32);
    fn delete_texture(&mut self, id: ImageId);

    fn texture_flags(&self, id: ImageId) -> ImageFlags;
    fn texture_size(&self, id: ImageId) -> (u32, u32);
    fn texture_type(&self, id: ImageId) -> Option<TextureType>;
}

#[derive(Debug)]
pub enum Flavor {
    ConvexFill {
        params: Params
    },
    ConcaveFill {
        fill_params: Params,
        stroke_params: Params,
    },
    Stroke {
        params: Params
    },
    StencilStroke {
        pass1: Params,
        pass2: Params
    },
    Triangles {
        params: Params
    },
}

#[derive(Copy, Clone, Default)]
pub struct Drawable {
    fill_verts: Option<(usize, usize)>,
    stroke_verts: Option<(usize, usize)>,
}

pub struct Command {
    flavor: Flavor,
    drawables: Vec<Drawable>,
    triangles_verts: Option<(usize, usize)>,
    image: Option<ImageId>,
}

impl Command {
    pub fn new(flavor: Flavor) -> Self {
        Self {
            flavor: flavor,
            drawables: Default::default(),
            triangles_verts: Default::default(),
            image: Default::default(),
        }
    }
}

pub struct GpuRenderer<T> {
    stencil_strokes: bool,
    backend: T,
    cmds: Vec<Command>,
    verts: Vec<Vertex>,
    fringe_width: f32
}

impl<T: GpuRendererBackend> GpuRenderer<T> {

    pub fn new(backend: T) -> Self {
        Self {
            stencil_strokes: true,
            backend: backend,
            cmds: Default::default(),
            verts: Default::default(),
            fringe_width: 1.0
        }
    }

}

impl<T: GpuRendererBackend> Renderer for GpuRenderer<T> {
    fn flush(&mut self) {
        self.backend.render(&self.verts, &self.cmds);
        self.cmds.clear();
        self.verts.clear();
    }

    fn clear_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        self.backend.clear_rect(x, y, width, height, color);
    }

    fn set_size(&mut self, width: u32, height: u32, dpi: f32) {
        // TODO: use dpi to calculate fringe_width, tes_tol and dist_tol
        self.backend.set_size(width, height, dpi);
    }

    fn fill(&mut self, paint: &Paint, scissor: &Scissor, path: &Path) {

        // TODO: don't hardcode tes_tol and dist_tol here
        let mut cache = CachedPath::new(path, 0.25, 0.01);

        if paint.shape_anti_alias() {
            cache.expand_fill(self.fringe_width, LineJoin::Miter, 2.4, self.fringe_width);
        } else {
            cache.expand_fill(0.0, LineJoin::Miter, 2.4, self.fringe_width);
        }

        let flavor = if cache.contours.len() == 1 && cache.contours[0].convexity == Convexity::Convex {
            let params = Params::new(&self.backend, paint, scissor, self.fringe_width, self.fringe_width, -1.0);

            Flavor::ConvexFill { params }
        } else {
            let mut fill_params = Params::default();
            fill_params.stroke_thr = -1.0;
            fill_params.shader_type = ShaderType::Simple.to_i32() as f32;//TODO to_f32 method

            let stroke_params = Params::new(&self.backend, paint, scissor, self.fringe_width, self.fringe_width, -1.0);

            Flavor::ConcaveFill { fill_params, stroke_params }
        };

        let mut cmd = Command::new(flavor);
        cmd.image = paint.image();

        let mut offset = self.verts.len();

        for contour in cache.contours {
            let mut drawable = Drawable::default();

            if !contour.fill.is_empty() {
                drawable.fill_verts = Some((offset, contour.fill.len()));
                self.verts.extend_from_slice(&contour.fill);
                offset += contour.fill.len();
            }

            if !contour.stroke.is_empty() {
                drawable.stroke_verts = Some((offset, contour.stroke.len()));
                self.verts.extend_from_slice(&contour.stroke);
                offset += contour.stroke.len();
            }

            cmd.drawables.push(drawable);
        }

        if let Flavor::ConcaveFill {..} = cmd.flavor {
            // Quad
            self.verts.push(Vertex::new(cache.bounds[2], cache.bounds[3], 0.5, 1.0));
            self.verts.push(Vertex::new(cache.bounds[2], cache.bounds[1], 0.5, 1.0));
            self.verts.push(Vertex::new(cache.bounds[0], cache.bounds[3], 0.5, 1.0));
            self.verts.push(Vertex::new(cache.bounds[0], cache.bounds[1], 0.5, 1.0));

            cmd.triangles_verts = Some((offset, 4));
        }

        self.cmds.push(cmd);
    }

    fn stroke(&mut self, paint: &Paint, scissor: &Scissor, path: &Path) {
        let tess_tol = 0.25;
        // TODO: don't hardcode tes_tol and dist_tol here
        let mut cache = CachedPath::new(path, tess_tol, 0.01);

        if paint.shape_anti_alias() {
            cache.expand_stroke(paint.stroke_width() * 0.5, self.fringe_width, paint.line_cap(), paint.line_join(), paint.miter_limit(), tess_tol);
        } else {
            cache.expand_stroke(paint.stroke_width() * 0.5, 0.0, paint.line_cap(), paint.line_join(), paint.miter_limit(), tess_tol);
        }

        let params = Params::new(&self.backend, paint, scissor, paint.stroke_width(), self.fringe_width, -1.0);

        let flavor = if self.stencil_strokes {
            let pass2 = Params::new(&self.backend, paint, scissor, paint.stroke_width(), self.fringe_width, 1.0 - 0.5/255.0);

            Flavor::StencilStroke { pass1: params, pass2 }
        } else {
            Flavor::Stroke { params }
        };

        let mut cmd = Command::new(flavor);
        cmd.image = paint.image();

        let mut offset = self.verts.len();

        for contour in cache.contours {
            let mut drawable = Drawable::default();

            if !contour.stroke.is_empty() {
                drawable.stroke_verts = Some((offset, contour.stroke.len()));
                self.verts.extend_from_slice(&contour.stroke);
                offset += contour.stroke.len();
            }

            cmd.drawables.push(drawable);
        }

        self.cmds.push(cmd);
    }

    fn triangles(&mut self, paint: &Paint, scissor: &Scissor, verts: &[Vertex]) {
        let mut params = Params::new(&self.backend, paint, scissor, 1.0, 1.0, -1.0);
        params.shader_type = ShaderType::Img.to_i32() as f32; // TODO:

        let mut cmd = Command::new(Flavor::Triangles { params });
        cmd.image = paint.image();
        cmd.triangles_verts = Some((self.verts.len(), verts.len()));
        self.cmds.push(cmd);

        self.verts.extend_from_slice(verts);
    }

    fn create_texture(&mut self, texture_type: TextureType, width: u32, height: u32, flags: ImageFlags) -> ImageId {
        self.backend.create_texture(texture_type, width, height, flags)
    }

    fn update_texture(&mut self, id: ImageId, image: &DynamicImage, x: u32, y: u32, w: u32, h: u32) {
        self.backend.update_texture(id, image, x, y, w, h);
    }

    fn delete_texture(&mut self, id: ImageId) {
        self.backend.delete_texture(id);
    }
}

// TODO: Rename those to make more sense - why do we have FillImage and Img?
#[derive(Copy, Clone)]
enum ShaderType {
    FillGradient,
    FillImage,
    Simple,
    Img
}

impl Default for ShaderType {
    fn default() -> Self { Self::Simple }
}

impl ShaderType {
    pub fn to_i32(self) -> i32 {
        match self {
            Self::FillGradient => 0,
            Self::FillImage => 1,
            Self::Simple => 2,
            Self::Img => 3,
        }
    }
}

#[derive(Copy, Clone, Default, Debug)]
pub struct Params {
    scissor_mat: [f32; 12],
    paint_mat: [f32; 12],
    inner_col: [f32; 4],
    outer_col: [f32; 4],
    scissor_ext: [f32; 2],
    scissor_scale: [f32; 2],
    extent: [f32; 2],
    radius: f32,
    feather: f32,
    stroke_mult: f32,
    stroke_thr: f32,
    shader_type: f32,
    tex_type: f32
}

impl Params {

    fn new<T: GpuRendererBackend>(backend: &T, paint: &Paint, scissor: &Scissor, width: f32, fringe: f32, stroke_thr: f32) -> Self {

        let mut params = Self::default();

        params.inner_col = paint.inner_color().premultiplied().to_array();
        params.outer_col = paint.outer_color().premultiplied().to_array();

        let (scissor_ext, scissor_scale) = if scissor.extent[0] < -0.5 || scissor.extent[1] < -0.5 {
            ([1.0, 1.0], [1.0, 1.0])
        } else {
            params.scissor_mat = scissor.transform.inversed().to_mat3x4();

            let scissor_scale = [
                (scissor.transform[0]*scissor.transform[0] + scissor.transform[2]*scissor.transform[2]).sqrt() / fringe,
                (scissor.transform[1]*scissor.transform[1] + scissor.transform[3]*scissor.transform[3]).sqrt() / fringe
            ];

            (scissor.extent, scissor_scale)
        };

        params.scissor_ext = scissor_ext;
        params.scissor_scale = scissor_scale;

        let extent = paint.extent();

        params.extent = extent;
        params.stroke_mult = (width*0.5 + fringe*0.5) / fringe;
        params.stroke_thr = stroke_thr;

        let inv_transform;

        if let Some(image_id) = paint.image() {

            let texture_flags = backend.texture_flags(image_id);

            if texture_flags.contains(ImageFlags::FLIP_Y) {
                let mut m1 = Transform2D::identity();
                m1.translate(0.0, extent[1] * 0.5);
                m1.multiply(&paint.transform());

                let mut m2 = Transform2D::identity();
                m2.scale(1.0, -1.0);
                m2.multiply(&m1);

                m1.translate(0.0, -extent[1] * 0.5);
                m1.multiply(&m2);

                inv_transform = m1.inversed();
            } else {
                inv_transform = paint.transform().inversed();
            }

            params.shader_type = ShaderType::FillImage.to_i32() as f32;// TODO: To f32 native method

            params.tex_type = match backend.texture_type(image_id) {
                Some(TextureType::Rgba) => if texture_flags.contains(ImageFlags::PREMULTIPLIED) { 0.0 } else { 1.0 },
                Some(TextureType::Alpha) => 2.0,
                _ => 0.0
            };
        } else {
            params.shader_type = ShaderType::FillGradient.to_i32() as f32;// TODO: To f32 native method
            params.radius = paint.radius();
            params.feather = paint.feather();

            inv_transform = paint.transform().inversed();
        }

        params.paint_mat = inv_transform.to_mat3x4();

        params
    }

}