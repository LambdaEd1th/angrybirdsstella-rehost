//! `sub_100031434`'s clipped-lines callback branch.

use super::UiTextTransform;
use crate::*;
use mlua::ObjectLike;

pub(super) fn draw(
    text: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
    transform: UiTextTransform,
    supplied_alpha: Option<f32>,
) -> LuaResult<()> {
    let lines = match text.raw_get::<Value>("lines")? {
        Value::Table(lines) => lines,
        _ => {
            return Err(runtime_error(
                "drawUITextNative clipped text lines must be table",
            ));
        }
    };
    if let Some(alpha) = supplied_alpha {
        render
            .lock()
            .expect("render bridge lock poisoned")
            .state
            .alpha = f64::from(alpha);
    }
    for index in 1.. {
        let line = match lines.raw_get::<Value>(index)? {
            Value::Nil => break,
            Value::Table(line) => line,
            _ => return Err(runtime_error("drawUITextNative clipped line must be table")),
        };
        // Only the draw-method lookup uses gettable. It may invoke __index;
        // 0319xx then reloads the receiver from the raw numeric array slot.
        let draw = line.get::<Value>("draw")?;
        let receiver = match lines.raw_get::<Value>(index)? {
            Value::Table(line) => line,
            _ => return Err(runtime_error("drawUITextNative clipped line must be table")),
        };
        let args = (
            receiver,
            transform.x,
            transform.y,
            transform.scale_x,
            transform.scale_y,
            transform.angle,
        );
        match draw {
            Value::Function(draw) => draw.call::<()>(args)?,
            Value::Table(draw) => draw.call::<()>(args)?,
            Value::UserData(draw) => draw.call::<()>(args)?,
            _ => {
                return Err(runtime_error(
                    "drawUITextNative clipped line draw must be callable",
                ));
            }
        }
    }
    if supplied_alpha.is_some() {
        render
            .lock()
            .expect("render bridge lock poisoned")
            .state
            .alpha = 1.0;
    }
    Ok(())
}
