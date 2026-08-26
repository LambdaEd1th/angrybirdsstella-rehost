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

    fn advance_z(&mut self, index: &NativeSceneRenderIndex) -> bool {
        let next = index.next_z_in_range(self.minimum_z, self.maximum_z, self.z);
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
    type Item = (i32, Option<Arc<str>>);

    fn next(&mut self) -> Option<Self::Item> {
        // One bridge acquisition corresponds to one resumed native tree walk.
        // The lock is still released before yielding to Lua, and the next call
        // re-reads the live vector length exactly like 0x10004C340.
        let render = Arc::clone(&self.render);
        let bridge = render.lock().expect("render bridge lock poisoned");
        let index = &bridge.scene_render_index;
        loop {
            let Some(z) = self.z else {
                if self.finished {
                    return None;
                }
                if !self.advance_z(index) {
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
                    let first = index.first_sheet(z);
                    let Some(first) = first else {
                        self.advance_z(index);
                        continue;
                    };
                    self.sheet = Some(first);
                    self.name_index = 0;
                    first
                }
            };
            let name = index.name_at(z, sheet, self.name_index);
            if let Some(name) = name {
                self.name_index += 1;
                return Some((z, Some(name)));
            }
            self.sheet = index.next_sheet(z, sheet);
            self.name_index = 0;
            if self.sheet.is_none() {
                self.advance_z(index);
            }
        }
    }
}
