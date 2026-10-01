//! Bounded window acquisition, independent of game-frame execution.

use anyhow::{Result, anyhow};
use wgpu::CurrentSurfaceTexture;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gpu) enum SurfaceRecovery {
    Reconfigure,
    Recreate,
}

#[derive(Debug)]
pub(super) struct AcquiredWindowFrame {
    pub(super) texture: Option<wgpu::SurfaceTexture>,
    pub(super) pending_recovery: Option<SurfaceRecovery>,
}

pub(super) fn acquire_window_frame<C>(
    context: &mut C,
    pending_recovery: Option<SurfaceRecovery>,
    mut acquire: impl FnMut(&mut C) -> Result<CurrentSurfaceTexture>,
    mut recover: impl FnMut(&mut C, SurfaceRecovery) -> Result<()>,
) -> Result<AcquiredWindowFrame> {
    // At most one recovery and two acquisitions per redraw. A second change
    // is remembered for the next redraw, rather than spinning on an unstable
    // surface or turning a temporary condition into a fatal game error.
    if let Some(recovery) = pending_recovery {
        recover(context, recovery)?;
        return classify(acquire(context)?);
    }
    let frame = classify(acquire(context)?)?;
    if frame.texture.is_none()
        && let Some(recovery) = frame.pending_recovery
    {
        recover(context, recovery)?;
        return classify(acquire(context)?);
    }
    Ok(frame)
}

fn classify(status: CurrentSurfaceTexture) -> Result<AcquiredWindowFrame> {
    let (texture, pending_recovery) = match status {
        CurrentSurfaceTexture::Success(texture) => (Some(texture), None),
        // Configure only after this texture has been presented and released.
        CurrentSurfaceTexture::Suboptimal(texture) => {
            (Some(texture), Some(SurfaceRecovery::Reconfigure))
        }
        CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => (None, None),
        CurrentSurfaceTexture::Outdated => (None, Some(SurfaceRecovery::Reconfigure)),
        CurrentSurfaceTexture::Lost => (None, Some(SurfaceRecovery::Recreate)),
        CurrentSurfaceTexture::Validation => {
            return Err(anyhow!("wgpu surface acquisition validation error"));
        }
    };
    Ok(AcquiredWindowFrame {
        texture,
        pending_recovery,
    })
}

#[cfg(test)]
mod tests;
