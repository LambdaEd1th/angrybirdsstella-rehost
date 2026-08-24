//! Clipper 6.2.1 `CleanPolygon`, including its `OutPt` ring traversal.

use super::NativeClipperPoint;

pub(super) fn clean_clipper_polygon(
    path: Vec<NativeClipperPoint>,
    distance: f64,
) -> Vec<NativeClipperPoint> {
    let mut ring = CleanRing::new(path);
    if ring.points.is_empty() {
        return Vec::new();
    }

    let distance_squared = distance * distance;
    let mut cursor = 0;
    while !ring.visited[cursor] && ring.next[cursor] != ring.previous[cursor] {
        let previous = ring.previous[cursor];
        let next = ring.next[cursor];
        if points_are_close(ring.points[cursor], ring.points[previous], distance_squared) {
            cursor = ring.exclude(cursor);
        } else if points_are_close(ring.points[previous], ring.points[next], distance_squared) {
            ring.exclude(next);
            cursor = ring.exclude(cursor);
        } else if slopes_near_collinear(
            ring.points[previous],
            ring.points[cursor],
            ring.points[next],
            distance_squared,
        ) {
            cursor = ring.exclude(cursor);
        } else {
            ring.visited[cursor] = true;
            cursor = ring.next[cursor];
        }
    }

    if ring.size < 3 {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(ring.size);
    for _ in 0..ring.size {
        result.push(ring.points[cursor]);
        cursor = ring.next[cursor];
    }
    result
}

struct CleanRing {
    points: Vec<NativeClipperPoint>,
    previous: Vec<usize>,
    next: Vec<usize>,
    visited: Vec<bool>,
    size: usize,
}

impl CleanRing {
    fn new(points: Vec<NativeClipperPoint>) -> Self {
        let size = points.len();
        let previous = (0..size).map(|index| (index + size - 1) % size).collect();
        let next = (0..size).map(|index| (index + 1) % size).collect();
        Self {
            points,
            previous,
            next,
            visited: vec![false; size],
            size,
        }
    }

    /// Exact index equivalent of Clipper 6.2.1 `ExcludeOp`.
    fn exclude(&mut self, cursor: usize) -> usize {
        let result = self.previous[cursor];
        let next = self.next[cursor];
        self.next[result] = next;
        self.previous[next] = result;
        self.visited[result] = false;
        self.size -= 1;
        result
    }
}

fn points_are_close(first: NativeClipperPoint, second: NativeClipperPoint, limit: f64) -> bool {
    let dx = first.x as f64 - second.x as f64;
    let dy = first.y as f64 - second.y as f64;
    dx * dx + dy * dy <= limit
}

fn slopes_near_collinear(
    first: NativeClipperPoint,
    second: NativeClipperPoint,
    third: NativeClipperPoint,
    limit: f64,
) -> bool {
    if (first.x - second.x).abs() > (first.y - second.y).abs() {
        if (first.x > second.x) == (first.x < third.x) {
            distance_from_line_squared(first, second, third) < limit
        } else if (second.x > first.x) == (second.x < third.x) {
            distance_from_line_squared(second, first, third) < limit
        } else {
            distance_from_line_squared(third, first, second) < limit
        }
    } else if (first.y > second.y) == (first.y < third.y) {
        distance_from_line_squared(first, second, third) < limit
    } else if (second.y > first.y) == (second.y < third.y) {
        distance_from_line_squared(second, first, third) < limit
    } else {
        distance_from_line_squared(third, first, second) < limit
    }
}

fn distance_from_line_squared(
    point: NativeClipperPoint,
    first: NativeClipperPoint,
    second: NativeClipperPoint,
) -> f64 {
    let a = (first.y - second.y) as f64;
    let b = (second.x - first.x) as f64;
    let mut c = a * first.x as f64 + b * first.y as f64;
    c = a * point.x as f64 + b * point.y as f64 - c;
    c * c / (a * a + b * b)
}
