use super::*;

pub(super) fn apply_sprite_shader(source: [u8; 4], shader: &SpriteShader) -> [u8; 4] {
    let mut color = source.map(|channel| f64::from(channel) / 255.0);
    let is_colorize = shader.name.starts_with("2d-sprite-colorize")
        || shader.name.starts_with("2d-sprite-silhouette")
        || shader.name.starts_with("2d-sprite-gold");
    if is_colorize {
        let grayscale = (color[0] + color[1] + color[2]) / 3.0;
        if shader.name.starts_with("2d-sprite-gold") {
            let highlight = shader.highlight * grayscale * grayscale;
            let inverse = 1.0 - grayscale;
            let luminance = 1.0 - inverse * inverse + shader.lightness * color[3];
            for (index, channel) in color.iter_mut().enumerate() {
                let base = if index == 3 {
                    source[3] as f64 / 255.0
                } else {
                    luminance
                };
                *channel = base * shader.diffuse[index] + highlight;
            }
        } else {
            let lightness = shader.lightness * color[3];
            for channel in &mut color[..3] {
                *channel = grayscale + (*channel - grayscale) * shader.saturation;
                *channel += lightness;
            }
            for (channel, diffuse) in color.iter_mut().zip(shader.diffuse) {
                *channel = channel.min(1.0) * diffuse;
            }
        }
    } else if shader.name.starts_with("2d-sprite-diffuse-modulate") {
        for (channel, diffuse) in color.iter_mut().zip(shader.diffuse) {
            *channel *= diffuse;
        }
    }
    color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}
