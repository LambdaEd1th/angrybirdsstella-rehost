//! Live three-level traversal performed by `sub_10004BAB4`.

use crate::*;

pub(super) struct NativeSceneWalk {
    render: Arc<Mutex<RenderBridge>>,
    minimum_z: i32,
    maximum_z: i32,
    z: Option<i32>,
    sheet: Option<u64>,
    name_index: usize,
    emit_z: bool,
    finished: bool,
}

impl NativeSceneWalk {
    pub(super) fn new(render: Arc<Mutex<RenderBridge>>, bounds: (i32, i32)) -> Self {
        Self {
            render,
            minimum_z: bounds.0,
            maximum_z: bounds.1,
            z: None,
            sheet: None,
            name_index: 0,
            emit_z: false,
            finished: false,
        }
    }

    fn advance_z(&mut self) -> bool {
        let next = self
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .scene_render_index
            .next_z_in_range(self.minimum_z, self.maximum_z, self.z);
        self.z = next;
        self.sheet = None;
        self.name_index = 0;
        self.emit_z = next.is_some();
        self.finished = next.is_none();
        next.is_some()
    }
}

impl Iterator for NativeSceneWalk {
    /// `None` names mark every persistent outer z node, including empty ones.
    /// Names are then fetched by live vector index so callback mutations have
    /// the same shift/append behavior as Purple's pointer loop.
    type Item = (i32, Option<String>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some(z) = self.z else {
                if self.finished {
                    return None;
                }
                if !self.advance_z() {
                    return None;
                }
                continue;
            };
            if self.emit_z {
                self.emit_z = false;
                return Some((z, None));
            }
            let sheet = match self.sheet {
                Some(sheet) => sheet,
                None => {
                    let first = self
                        .render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .scene_render_index
                        .first_sheet(z);
                    let Some(first) = first else {
                        self.advance_z();
                        continue;
                    };
                    self.sheet = Some(first);
                    self.name_index = 0;
                    first
                }
            };
            let name = self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .scene_render_index
                .name_at(z, sheet, self.name_index);
            if let Some(name) = name {
                self.name_index += 1;
                return Some((z, Some(name)));
            }
            self.sheet = self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .scene_render_index
                .next_sheet(z, sheet);
            self.name_index = 0;
            if self.sheet.is_none() {
                self.advance_z();
            }
        }
    }
}
