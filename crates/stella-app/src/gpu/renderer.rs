//! wgpu device, surface, render-pass, capture, and texture synchronization stage.

mod capture;
pub(in crate::gpu) mod initialization;
mod pass;
mod presentation;
mod streams;
mod textures;
pub(in crate::gpu) mod window_overlay;
