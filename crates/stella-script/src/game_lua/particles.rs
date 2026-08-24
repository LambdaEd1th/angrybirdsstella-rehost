//! Native `Particles` object, cached definitions, random source, and members.

mod definition;
mod model;
mod random;
mod spawn;
mod system;
mod theme;

pub(crate) use definition::ParticleDefinition;
pub(crate) use model::Particle;
pub(crate) use random::NativeParticleRandom;
pub(crate) use spawn::{
    ParticleEmitter, emit_particles, prepare_particle_emitter, spawn_particles,
};
pub(crate) use system::NativeParticles;
pub(crate) use theme::NativeThemeParticles;
