//! Strict `drawUITextNative` entry and its recovered control-flow branches.

mod clipped;
mod ordinary;

use crate::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct UiTextTransform {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) scale_x: f32,
    pub(super) scale_y: f32,
    pub(super) angle: f32,
    pub(super) cosine: f32,
    pub(super) sine: f32,
}

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let text_render_bridge = Arc::clone(&render);
    let text_render_resources = Arc::clone(&resource_runtime);
    let text_render_locales = Arc::clone(&locale_runtime);
    globals.set(
        "drawUITextNative",
        lua.create_function(move |_, args: MultiValue| {
            trace_call(&args);
            let text = args
                .front()
                .and_then(value_table)
                .ok_or_else(|| runtime_error("drawUITextNative argument 1 must be table"))?;
            let parent_x = value_number_at(&args, 1)
                .ok_or_else(|| runtime_error("drawUITextNative argument 2 must be number"))?
                as f32;
            let parent_y = value_number_at(&args, 2)
                .ok_or_else(|| runtime_error("drawUITextNative argument 3 must be number"))?
                as f32;
            let argument_count = args.len();
            // sub_100031434 reads the two parent scales only when both
            // arguments are present. With exactly four arguments, argument 4
            // is deliberately ignored without even checking its type.
            let (parent_scale_x, parent_scale_y) = if argument_count >= 5 {
                (
                    value_number_at(&args, 3).ok_or_else(|| {
                        runtime_error("drawUITextNative argument 4 must be number")
                    })? as f32,
                    value_number_at(&args, 4).ok_or_else(|| {
                        runtime_error("drawUITextNative argument 5 must be number")
                    })? as f32,
                )
            } else {
                (1.0_f32, 1.0_f32)
            };
            let parent_angle = if argument_count >= 6 {
                value_number_at(&args, 5)
                    .ok_or_else(|| runtime_error("drawUITextNative argument 6 must be number"))?
                    as f32
            } else {
                0.0_f32
            };
            let supplied_alpha =
                if argument_count >= 7 {
                    Some(value_number_at(&args, 6).ok_or_else(|| {
                        runtime_error("drawUITextNative argument 7 must be number")
                    })? as f32)
                } else {
                    None
                };

            if !matches!(text.get::<Value>("visible")?, Value::Boolean(true)) {
                return Ok(());
            }
            // sub_100031594..0x10003181C evaluates the two trig functions and
            // every table scalar in float32 S registers. The native fetches X
            // and Y twice because each rotated component is assembled after a
            // separate Lua-table lookup; preserve that observable order too.
            let cosine = parent_angle.cos();
            let local_x_for_x = table_required_number(&text, "x", "drawUITextNative")? as f32;
            let sine = parent_angle.sin();
            let local_y_for_x = table_required_number(&text, "y", "drawUITextNative")? as f32;
            let local_x_for_y = table_required_number(&text, "x", "drawUITextNative")? as f32;
            let local_y_for_y = table_required_number(&text, "y", "drawUITextNative")? as f32;
            let local_scale_x = table_required_number(&text, "scaleX", "drawUITextNative")? as f32;
            let local_scale_y = table_required_number(&text, "scaleY", "drawUITextNative")? as f32;
            select_font(&text, &text_render_resources)?;

            // 0x1000317F0..0x10003181C: three FMULs followed by FNMSUB for X,
            // three FMULs followed by FMADD for Y, then two scale FMULs. Using
            // mul_add at the same final step retains Purple's single rounding.
            let cosine_local_x = cosine * local_x_for_x;
            let scaled_sine_local_y = parent_scale_y * (sine * local_y_for_x);
            let offset_x = parent_scale_x.mul_add(cosine_local_x, -scaled_sine_local_y);
            let sine_local_x = sine * local_x_for_y;
            let scaled_cosine_local_y = parent_scale_y * (cosine * local_y_for_y);
            let offset_y = parent_scale_x.mul_add(sine_local_x, scaled_cosine_local_y);
            let transform = UiTextTransform {
                x: parent_x + offset_x,
                y: parent_y + offset_y,
                scale_x: parent_scale_x * local_scale_x,
                scale_y: parent_scale_y * local_scale_y,
                angle: parent_angle,
                cosine,
                sine,
            };

            if matches!(text.get::<Value>("clipped")?, Value::Boolean(true)) {
                return clipped::draw(&text, &text_render_bridge, transform, supplied_alpha);
            }
            ordinary::draw(
                &text,
                &text_render_bridge,
                &text_render_resources,
                &text_render_locales,
                &data_root,
                transform,
                supplied_alpha,
            )
        })?,
    )?;

    Ok(())
}

fn select_font(text: &mlua::Table, resources: &Arc<Mutex<ResourceRuntime>>) -> LuaResult<()> {
    let font_request = match text.get::<Value>("font")? {
        Value::String(font) => font.to_str()?.to_owned(),
        _ => "FONT_BASIC_SPACE".to_owned(),
    };
    // ResourceManager::useFont keeps the prior font when the requested object
    // is absent. Selection happens before either draw branch in the native.
    let mut resources = resources.lock().expect("resource runtime lock poisoned");
    if resources.bitmap_fonts.contains(&font_request)
        || resources.system_fonts.contains_key(&font_request)
    {
        resources.current_font = Some(font_request);
    }
    Ok(())
}

fn trace_call(args: &MultiValue) {
    if std::env::var_os("STELLA_TRACE_NATIVE").is_none() {
        return;
    }
    let fields = args
        .front()
        .and_then(value_table)
        .map(|text| {
            [
                "name", "text", "font", "group", "hanchor", "vanchor", "color", "width", "height",
                "alpha", "x", "y", "scaleX", "scaleY",
            ]
            .into_iter()
            .filter_map(|key| {
                text.get::<Value>(key)
                    .ok()
                    .filter(|value| !matches!(value, Value::Nil))
                    .map(|value| format!("{key}={}", describe_value(&value)))
            })
            .collect::<Vec<_>>()
            .join(" ")
        })
        .unwrap_or_default();
    let values = args
        .iter()
        .map(describe_value)
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "native drawUITextNative argc={} args=[{values}] {fields}",
        args.len()
    );
}
