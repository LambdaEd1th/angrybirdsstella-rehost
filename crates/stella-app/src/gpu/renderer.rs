//! wgpu device, surface, render-pass, capture, and texture synchronization stage.

pub(in crate::gpu) mod initialization;
mod pass;
mod presentation;
mod streams;
mod textures;
