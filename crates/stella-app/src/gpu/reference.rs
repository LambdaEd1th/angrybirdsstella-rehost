//! Test-only rasterizer of the submitted clip-space stream. Geometry generation
//! is shared with wgpu; clipping, coverage, interpolation and sampling are not.

use super::*;

#[derive(Clone, Copy, Debug)]
struct ClipVertex {
    position: [f64; 4],
    uv: [f64; 2],
    source: [f64; 2],
}

impl From<GpuVertex> for ClipVertex {
    fn from(vertex: GpuVertex) -> Self {
        Self {
            position: vertex.clip_position.map(f64::from),
            uv: vertex.uv.map(f64::from),
            source: vertex.source.map(f64::from),
        }
    }
}

impl ClipVertex {
    fn interpolate(self, other: Self, t: f64) -> Self {
        Self {
            position: std::array::from_fn(|i| {
                self.position[i] + (other.position[i] - self.position[i]) * t
            }),
            uv: std::array::from_fn(|i| self.uv[i] + (other.uv[i] - self.uv[i]) * t),
            source: std::array::from_fn(|i| {
                self.source[i] + (other.source[i] - self.source[i]) * t
            }),
        }
    }

    fn plane_distance(self, plane: usize) -> f64 {
        let [x, y, z, w] = self.position;
        match plane {
            0 => x + w,
            1 => w - x,
            2 => y + w,
            3 => w - y,
            4 => z,
            5 => w - z,
            _ => unreachable!(),
        }
    }
}

fn clip_triangle(triangle: [ClipVertex; 3]) -> Vec<ClipVertex> {
    if triangle.iter().any(|vertex| {
        vertex
            .position
            .iter()
            .chain(&vertex.uv)
            .chain(&vertex.source)
            .any(|value| !value.is_finite())
    }) {
        return Vec::new();
    }
    let mut polygon = triangle.to_vec();
    for plane in 0..6 {
        let Some(mut previous) = polygon.last().copied() else {
            break;
        };
        let mut clipped = Vec::with_capacity(polygon.len() + 1);
        let mut previous_distance = previous.plane_distance(plane);
        for current in polygon {
            let current_distance = current.plane_distance(plane);
            if (current_distance >= 0.0) != (previous_distance >= 0.0) {
                let t = previous_distance / (previous_distance - current_distance);
                clipped.push(previous.interpolate(current, t));
            }
            if current_distance >= 0.0 {
                clipped.push(current);
            }
            previous = current;
            previous_distance = current_distance;
        }
        polygon = clipped;
    }
    polygon
}

impl PreparedFrame {
    /// Composite onto an existing RGB target, retaining native immediate order.
    /// Each attachment write is rounded to RGBA8 just like the game texture.
    pub(crate) fn render_reference(
        &self,
        assets: &mut AssetCatalog,
        target: &mut [u32],
    ) -> Result<()> {
        let resolution = self.resolution;
        if target.len() != resolution.width as usize * resolution.height as usize {
            return Err(anyhow!(
                "reference target does not match prepared resolution"
            ));
        }
        let mut output = RgbaImage::from_fn(resolution.width, resolution.height, |x, y| {
            let color = target[(y * resolution.width + x) as usize];
            image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255])
        });
        let mut textures = HashMap::<String, Arc<RgbaImage>>::new();
        textures.insert(
            WHITE_TEXTURE.to_owned(),
            Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]))),
        );
        // A generation produced later in this frame does not exist yet. Older
        // generations, however, must be loaded from the persistent CPU cache;
        // skipping every capture-looking name loses cross-frame captures.
        let pending_captures = self
            .operations
            .iter()
            .filter_map(|operation| match operation {
                PreparedOperation::Capture(name) => Some(name.as_str()),
                PreparedOperation::Draw(_) => None,
            })
            .collect::<HashSet<_>>();
        for name in &self.required_textures {
            if let Some(texture) = self.transient_textures.get(name) {
                textures.insert(name.clone(), Arc::new(texture.image.clone()));
            } else if !pending_captures.contains(name.as_str()) {
                textures.insert(name.clone(), reference_texture(assets, name)?);
            }
        }
        for operation in &self.operations {
            match operation {
                PreparedOperation::Capture(name) => {
                    // GL captures its lower-left framebuffer row into texture
                    // row zero. glCopyTexImage2D(GL_RGB) discards framebuffer
                    // alpha even if the existing Image retains an RGBA format.
                    let captured =
                        RgbaImage::from_fn(resolution.width, resolution.height, |x, y| {
                            let pixel = output.get_pixel(x, resolution.height - y - 1);
                            image::Rgba([pixel[0], pixel[1], pixel[2], 255])
                        });
                    textures.insert(name.clone(), Arc::new(captured.clone()));
                    assets.textures.insert(
                        name.clone(),
                        TextureAsset::new(captured, self.capture_formats[name]),
                    );
                }
                PreparedOperation::Draw(index) => {
                    let draw = &self.draws[*index];
                    let (base_name, fill_name) = &self.texture_pairs[draw.texture_pair];
                    for name in [base_name, fill_name] {
                        if !textures.contains_key(name) {
                            textures.insert(name.clone(), reference_texture(assets, name)?);
                        }
                    }
                    let material = Material {
                        base: &textures[base_name],
                        fill: &textures[fill_name],
                        program: draw.program,
                    };
                    for triangle in self.vertices
                        [draw.vertices.start as usize..draw.vertices.end as usize]
                        .as_chunks::<3>()
                        .0
                    {
                        let uniform = self.uniforms[triangle[0].draw_index as usize];
                        let polygon = clip_triangle([
                            triangle[0].into(),
                            triangle[1].into(),
                            triangle[2].into(),
                        ]);
                        for index in 1..polygon.len().saturating_sub(1) {
                            raster_triangle(
                                [polygon[0], polygon[index], polygon[index + 1]],
                                uniform,
                                &material,
                                draw.scissor,
                                &mut output,
                            );
                        }
                    }
                }
            }
        }
        // A replay/GPU comparison may still need an earlier generation from
        // this stream. Retain those until a subsequent frame no longer uses
        // them; the current logical bindings persist even across resize.
        let live_captures = assets
            .captures
            .bindings
            .values()
            .map(|texture| texture.source.clone())
            .collect::<HashSet<_>>();
        assets.textures.retain(|name, _| {
            !name.starts_with("<capture-generation:")
                || live_captures.contains(name)
                || self.required_textures.contains(name)
        });
        for (pixel, color) in target.iter_mut().zip(output.pixels()) {
            *pixel = (u32::from(color[0]) << 16) | (u32::from(color[1]) << 8) | u32::from(color[2]);
        }
        Ok(())
    }
}

fn reference_texture(assets: &mut AssetCatalog, physical_source: &str) -> Result<Arc<RgbaImage>> {
    // Preparing the full stream has already advanced logical bindings to the
    // final generation. Earlier draw pairs are physical snapshots, so loading
    // an original atlas here must not resolve that logical name a second time.
    if let Some(texture) = assets.textures.get(physical_source) {
        return Ok(Arc::new(texture.image.clone()));
    }
    Ok(Arc::new(assets.texture(physical_source)?.image.clone()))
}

fn edge(a: [f64; 2], b: [f64; 2], point: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
}

fn top_left(a: [f64; 2], b: [f64; 2]) -> bool {
    b[1] < a[1] || (b[1] == a[1] && b[0] > a[0])
}

fn raster_triangle(
    mut triangle: [ClipVertex; 3],
    uniform: DrawUniform,
    material: &Material<'_>,
    scissor: Option<[u32; 4]>,
    target: &mut RgbaImage,
) {
    if triangle.iter().any(|vertex| vertex.position[3] <= 0.0) {
        return;
    }
    let width = target.width();
    let height = target.height();
    let mut screen = triangle.map(|vertex| {
        let [x, y, _, w] = vertex.position;
        [
            (x / w + 1.0) * f64::from(width) * 0.5,
            (1.0 - y / w) * f64::from(height) * 0.5,
        ]
    });
    let mut area = edge(screen[0], screen[1], screen[2]);
    if !area.is_finite() || area == 0.0 {
        return;
    }
    if area < 0.0 {
        triangle.swap(1, 2);
        screen.swap(1, 2);
        area = -area;
    }
    let [clip_x, clip_y, clip_width, clip_height] = scissor.unwrap_or([0, 0, width, height]);
    let minimum_x = screen.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let maximum_x = screen
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let minimum_y = screen.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let maximum_y = screen
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let left = (minimum_x.floor().max(0.0) as u32).max(clip_x);
    let top = (minimum_y.floor().max(0.0) as u32).max(clip_y);
    let right = (maximum_x.ceil().max(0.0) as u32)
        .min(width)
        .min(clip_x.saturating_add(clip_width));
    let bottom = (maximum_y.ceil().max(0.0) as u32)
        .min(height)
        .min(clip_y.saturating_add(clip_height));
    let inclusions = [
        top_left(screen[1], screen[2]),
        top_left(screen[2], screen[0]),
        top_left(screen[0], screen[1]),
    ];
    for y in top..bottom {
        for x in left..right {
            let point = [f64::from(x) + 0.5, f64::from(y) + 0.5];
            let edges = [
                edge(screen[1], screen[2], point),
                edge(screen[2], screen[0], point),
                edge(screen[0], screen[1], point),
            ];
            if edges
                .iter()
                .zip(inclusions)
                .any(|(distance, inclusive)| *distance < 0.0 || (*distance == 0.0 && !inclusive))
            {
                continue;
            }
            let weights =
                std::array::from_fn::<_, 3, _>(|i| edges[i] / area / triangle[i].position[3]);
            let divisor = weights.iter().sum::<f64>();
            let uv = std::array::from_fn(|i| {
                (0..3).map(|j| weights[j] * triangle[j].uv[i]).sum::<f64>() / divisor
            });
            let source = std::array::from_fn(|i| {
                (0..3)
                    .map(|j| weights[j] * triangle[j].source[i])
                    .sum::<f64>()
                    / divisor
            });
            let color = material.shade(uniform, uv, source);
            let destination = target.get_pixel(x, y).0.map(|c| f64::from(c) / 255.0);
            let (source_factor, destination_factor) = match material.program {
                NativeProgram::Plain | NativeProgram::Sprite => (1.0, 0.0),
                NativeProgram::SpriteAlpha => (1.0, 1.0 - color[3]),
                NativeProgram::PlainAlpha | NativeProgram::SpriteAlphaMasked => {
                    (color[3], 1.0 - color[3])
                }
            };
            target.put_pixel(
                x,
                y,
                image::Rgba(std::array::from_fn(|i| {
                    ((color[i] * source_factor + destination[i] * destination_factor)
                        .clamp(0.0, 1.0)
                        * 255.0)
                        .round() as u8
                })),
            );
        }
    }
}

struct Material<'a> {
    base: &'a RgbaImage,
    fill: &'a RgbaImage,
    program: NativeProgram,
}

impl Material<'_> {
    fn shade(&self, uniform: DrawUniform, uv: [f64; 2], source: [f64; 2]) -> [f64; 4] {
        let source_mode = (uniform.header[2] + 0.5) as u32;
        let diffuse = uniform.diffuse.map(f64::from);
        let mut color = match source_mode {
            2 => diffuse,
            3 => sample(self.fill, uv, true),
            1 => {
                let mask = sample(self.base, uv, false);
                let scale = f64::from(uniform.header[1]);
                let magnitude = scale.abs().max(0.000001);
                let scale = if scale >= 0.0 { magnitude } else { -magnitude };
                let fill_uv = std::array::from_fn(|i| {
                    source[i] / scale / f64::from(uniform.fill[i]).max(1.0)
                });
                let mut color = sample(self.fill, fill_uv, true);
                color[3] *= mask[3];
                color
            }
            _ => sample(self.base, uv, false),
        };
        let shader_mode = (uniform.header[3] + 0.5) as u32;
        if source_mode != 2 && shader_mode != 0 {
            if shader_mode == 4 {
                color = std::array::from_fn(|i| color[i] * diffuse[i]);
            } else {
                let grayscale = (color[0] + color[1] + color[2]) * f64::from(0.333_f32);
                let [lightness, saturation, highlight, _] = uniform.params.map(f64::from);
                if shader_mode == 3 {
                    let shine = highlight * grayscale * grayscale;
                    let luminance = 1.0 - (1.0 - grayscale).powi(2) + lightness * color[3];
                    color = std::array::from_fn(|i| {
                        (if i == 3 { color[3] } else { luminance }) * diffuse[i] + shine
                    });
                } else {
                    let lightness = lightness * color[3];
                    for i in 0..4 {
                        let gray_channel = if i == 3 { color[3] } else { grayscale };
                        let mixed = gray_channel * (1.0 - saturation) + color[i] * saturation;
                        let light = if i == 3 { 0.0 } else { lightness };
                        color[i] = if shader_mode == 1 {
                            mixed * diffuse[i] + light
                        } else {
                            (mixed + light).min(1.0) * diffuse[i]
                        };
                    }
                }
            }
        }
        color.map(|channel| (channel * f64::from(uniform.header[0])).clamp(0.0, 1.0))
    }
}

fn sample(image: &RgbaImage, uv: [f64; 2], repeat: bool) -> [f64; 4] {
    let coordinate = [
        uv[0] * f64::from(image.width()) - 0.5,
        uv[1] * f64::from(image.height()) - 0.5,
    ];
    let low = coordinate.map(f64::floor);
    let fraction = [coordinate[0] - low[0], coordinate[1] - low[1]];
    let pixel = |x: f64, y: f64| {
        let resolve = |value: f64, size: u32| {
            if repeat {
                value.rem_euclid(f64::from(size)) as u32
            } else {
                value.clamp(0.0, f64::from(size.saturating_sub(1))) as u32
            }
        };
        image
            .get_pixel(resolve(x, image.width()), resolve(y, image.height()))
            .0
    };
    let corners = [
        pixel(low[0], low[1]),
        pixel(low[0] + 1.0, low[1]),
        pixel(low[0], low[1] + 1.0),
        pixel(low[0] + 1.0, low[1] + 1.0),
    ];
    std::array::from_fn(|i| {
        let top =
            f64::from(corners[0][i]) * (1.0 - fraction[0]) + f64::from(corners[1][i]) * fraction[0];
        let bottom =
            f64::from(corners[2][i]) * (1.0 - fraction[0]) + f64::from(corners[3][i]) * fraction[0];
        (top * (1.0 - fraction[1]) + bottom * fraction[1]) / 255.0
    })
}

#[cfg(test)]
mod tests;
