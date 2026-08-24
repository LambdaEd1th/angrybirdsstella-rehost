//! `sub_100031434`'s clipped-lines callback branch.

use super::UiTextTransform;
use crate::*;

pub(super) fn draw(
    text: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
    transform: UiTextTransform,
    supplied_alpha: Option<f64>,
) -> LuaResult<()> {
    let lines = match text.get::<Value>("lines")? {
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
            .alpha = alpha;
    }
    let callback_result = (|| -> LuaResult<()> {
        for index in 1.. {
            let line = match lines.get::<Value>(index)? {
                Value::Nil => break,
                Value::Table(line) => line,
                _ => {
                    return Err(runtime_error("drawUITextNative clipped line must be table"));
                }
            };
            let draw = match line.get::<Value>("draw")? {
                Value::Function(draw) => draw,
                _ => {
                    return Err(runtime_error(
                        "drawUITextNative clipped line draw must be function",
                    ));
                }
            };
            draw.call::<()>((
                line,
                transform.x,
                transform.y,
                transform.scale_x,
                transform.scale_y,
                transform.angle,
            ))?;
        }
        Ok(())
    })();
    if supplied_alpha.is_some() {
        render
            .lock()
            .expect("render bridge lock poisoned")
            .state
            .alpha = 1.0;
    }
    callback_result
}
