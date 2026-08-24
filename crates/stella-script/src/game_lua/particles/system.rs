//! `Particles` allocation constructed by `sub_10008E160`.

use crate::*;

#[derive(Debug)]
pub(crate) struct NativeParticles {
    /// Packed 0x68-byte records at native offsets `+0x40..+0x50`.
    pub(crate) particles: Vec<Particle>,
    /// Definition-name map rooted at native offset `+0x60`.
    pub(crate) definitions: BTreeMap<String, ParticleDefinition>,
    /// Menu/notification draw scale at native offset `+0x38`.
    pub(crate) scale: f32,
}

impl Default for NativeParticles {
    fn default() -> Self {
        Self {
            particles: Vec::new(),
            definitions: BTreeMap::new(),
            scale: 1.0,
        }
    }
}

impl NativeParticles {
    /// Particles vtable slot `+0x28`, `sub_100091544`.
    pub(crate) fn remap_infinite_level_limits(&mut self, current: [f32; 4], old: [f32; 4]) {
        const PHYSICS_TO_FRAMEBUFFER_SCALE: f32 = 0.05_f32;

        let old_width = old[1] - old[0];
        let old_height = old[3] - old[2];
        let old_center_x = (old_width / PHYSICS_TO_FRAMEBUFFER_SCALE)
            .mul_add(0.5_f32, old[0] / PHYSICS_TO_FRAMEBUFFER_SCALE);
        let old_center_y = (old_height / PHYSICS_TO_FRAMEBUFFER_SCALE)
            .mul_add(0.5_f32, old[2] / PHYSICS_TO_FRAMEBUFFER_SCALE);
        let horizontal_ratio = (current[1] - current[0]) / old_width;
        let vertical_ratio = (current[3] - current[2]) / old_height;

        for particle in &mut self.particles {
            if particle.lifetime == -1.0_f32 {
                particle.x = horizontal_ratio.mul_add(particle.x - old_center_x, old_center_x);
                particle.y = vertical_ratio.mul_add(particle.y - old_center_y, old_center_y);
            }
        }
    }
}
