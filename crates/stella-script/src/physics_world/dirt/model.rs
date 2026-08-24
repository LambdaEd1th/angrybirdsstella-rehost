use crate::*;

#[derive(Debug, Clone)]
pub(crate) struct DirtComponent {
    textures: DirtTextures,
    pub(crate) fixture_density: f64,
    pub(crate) fixture_friction: f64,
    pub(crate) fixture_restitution: f64,
    pub(crate) background_paths: Vec<Vec<(f64, f64)>>,
    pub(crate) foreground_paths: Vec<Vec<(f64, f64)>>,
}

#[derive(Debug, Clone)]
pub(crate) struct DirtTextures {
    pub(crate) background: String,
    pub(crate) foreground: String,
    pub(crate) background_binding: MaskedTextureBinding,
    pub(crate) foreground_binding: MaskedTextureBinding,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DirtHole {
    pub(crate) local_x: f64,
    pub(crate) local_y: f64,
    pub(crate) radius: f64,
}

impl DirtComponent {
    pub(crate) fn from_object(
        object: &SceneObject,
        textures: DirtTextures,
        fixture_density: f64,
        fixture_friction: f64,
        fixture_restitution: f64,
    ) -> Option<Self> {
        let path = match &object.collision_shape {
            CollisionShape::Box { width, height } => vec![
                (-width * 0.5, -height * 0.5),
                (width * 0.5, -height * 0.5),
                (width * 0.5, height * 0.5),
                (-width * 0.5, height * 0.5),
            ],
            CollisionShape::Polygon { vertices, .. } => vertices.clone(),
            CollisionShape::Line { .. } | CollisionShape::Circle { .. } | CollisionShape::None => {
                return None;
            }
        };
        // sub_10001F98C pushes a direct copy of RenderObjectData+0x168 into
        // both DrawablePolygon paths. Clipper's 1000x integer conversion does
        // not occur until the first collision reaches sub_100020D70.
        let path = path
            .into_iter()
            .map(|(x, y)| (f64::from(x as f32), f64::from(y as f32)))
            .collect::<Vec<_>>();
        (path.len() >= 3).then(|| Self {
            textures,
            fixture_density,
            fixture_friction,
            fixture_restitution,
            background_paths: vec![path.clone()],
            foreground_paths: vec![path],
        })
    }

    pub(crate) fn cut(&mut self, hole: DirtHole) {
        self.foreground_paths = native_dirt_difference(&self.foreground_paths, hole);
    }

    pub(crate) fn foreground_fixtures(&self) -> Vec<Vec<(f64, f64)>> {
        triangulate_dirt_paths(&self.foreground_paths)
            .into_iter()
            .flatten()
            .map(|triangle| triangle.vertices.into_iter().map(|[x, y]| (x, y)).collect())
            .collect()
    }

    pub(crate) fn render_command(&self) -> DirtRenderCommand {
        DirtRenderCommand {
            background_texture: self.textures.background.clone(),
            foreground_texture: self.textures.foreground.clone(),
            background_texture_binding: self.textures.background_binding.clone(),
            foreground_texture_binding: self.textures.foreground_binding.clone(),
            background_triangles: triangulate_dirt_paths(&self.background_paths),
            foreground_triangles: triangulate_dirt_paths(&self.foreground_paths),
        }
    }
}
