//! Native chain-track state and closest-edge projection.

#[derive(Debug, Clone)]
pub(crate) struct ClosestTrackEdge {
    pub(crate) index: i32,
    pub(crate) start: (f64, f64),
    pub(crate) end: (f64, f64),
    pub(crate) angle: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct PhysicsTrack {
    pub(crate) object: String,
    pub(crate) points: Vec<(f64, f64)>,
    pub(crate) _open_ended: bool,
    pub(crate) rotate_block: bool,
    pub(crate) current_segment: i32,
    pub(crate) edge_start: (f64, f64),
    pub(crate) edge_end: (f64, f64),
    pub(crate) angle: f64,
    pub(crate) impulse_x: f64,
    pub(crate) impulse_y: f64,
}

impl PhysicsTrack {
    pub(crate) fn native_closest_edge(&self, position: (f64, f64)) -> Option<ClosestTrackEdge> {
        let point_x = position.0 as f32;
        let point_y = position.1 as f32;
        let mut best_distance = f32::INFINITY;
        let mut best_edge = None::<ClosestTrackEdge>;
        for (index, edge) in self.points.windows(2).enumerate() {
            let start_x = edge[0].0 as f32;
            let start_y = edge[0].1 as f32;
            let end_x = edge[1].0 as f32;
            let end_y = edge[1].1 as f32;
            let delta_x = end_x - start_x;
            let delta_y = end_y - start_y;
            let length_squared = delta_x.mul_add(delta_x, delta_y * delta_y);
            if length_squared == 0.0 {
                continue;
            }
            let projection = ((point_x - start_x).mul_add(delta_x, (point_y - start_y) * delta_y)
                / length_squared)
                .clamp(0.0, 1.0);
            let closest_x = delta_x.mul_add(projection, start_x);
            let closest_y = delta_y.mul_add(projection, start_y);
            let distance_x = point_x - closest_x;
            let distance_y = point_y - closest_y;
            let distance_squared = distance_x.mul_add(distance_x, distance_y * distance_y);
            // sub_10085D364 uses a strict comparison, preserving the first
            // chain child when two edges are equally near.
            if distance_squared < best_distance {
                best_distance = distance_squared;
                best_edge = Some(ClosestTrackEdge {
                    index: index as i32,
                    start: (f64::from(start_x), f64::from(start_y)),
                    end: (f64::from(end_x), f64::from(end_y)),
                    angle: f64::from(delta_y.atan2(delta_x)),
                });
            }
        }
        best_edge
    }

    pub(crate) fn native_current_angle(&self, position: (f64, f64)) -> f64 {
        self.native_closest_edge(position)
            .map(|edge| edge.angle)
            .unwrap_or(0.0)
    }
}
