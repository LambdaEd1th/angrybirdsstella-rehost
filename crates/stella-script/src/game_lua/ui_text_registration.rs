//! Strict `drawUITextNative` entry and its recovered control-flow branches.

mod clipped;
mod ordinary;

use crate::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct UiTextTransform {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) scale_x: f64,
    pub(super) scale_y: f64,
    pub(super) angle: f64,
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
                .ok_or_else(|| runtime_error("drawUITextNative argument 2 must be number"))?;
            let parent_y = value_number_at(&args, 2)
                .ok_or_else(|| runtime_error("drawUITextNative argument 3 must be number"))?;
            let argument_count = args.len();
            // sub_100031434 reads the two parent scales only when both
            // arguments are present. With exactly four arguments, argument 4
            // is deliberately ignored without even checking its type.
            let (parent_scale_x, parent_scale_y) = if argument_count >= 5 {
                (
                    value_number_at(&args, 3).ok_or_else(|| {
                        runtime_error("drawUITextNative argument 4 must be number")
                    })?,
                    value_number_at(&args, 4).ok_or_else(|| {
                        runtime_error("drawUITextNative argument 5 must be number")
                    })?,
                )
            } else {
                (1.0, 1.0)
            };
            let parent_angle = if argument_count >= 6 {
                value_number_at(&args, 5)
                    .ok_or_else(|| runtime_error("drawUITextNative argument 6 must be number"))?
            } else {
                0.0
            };
            let supplied_alpha =
                if argument_count >= 7 {
                    Some(value_number_at(&args, 6).ok_or_else(|| {
                        runtime_error("drawUITextNative argument 7 must be number")
                    })?)
                } else {
                    None
                };

            if !matches!(text.get::<Value>("visible")?, Value::Boolean(true)) {
                return Ok(());
            }
            let local_x = table_required_number(&text, "x", "drawUITextNative")?;
            let local_y = table_required_number(&text, "y", "drawUITextNative")?;
            let local_scale_x = table_required_number(&text, "scaleX", "drawUITextNative")?;
            let local_scale_y = table_required_number(&text, "scaleY", "drawUITextNative")?;
            select_font(&text, &text_render_resources)?;

            let cosine = parent_angle.cos();
            let sine = parent_angle.sin();
            let transform = UiTextTransform {
                x: parent_x + parent_scale_x * cosine * local_x - parent_scale_y * sine * local_y,
                y: parent_y + parent_scale_x * sine * local_x + parent_scale_y * cosine * local_y,
                scale_x: parent_scale_x * local_scale_x,
                scale_y: parent_scale_y * local_scale_y,
                angle: parent_angle,
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
