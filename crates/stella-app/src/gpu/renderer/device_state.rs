//! Whole-device loss is terminal for the current GPU resources, independently
//! of recoverable window surfaces. The callback only records the first loss;
//! application code observes it at its normal error/persistence boundary.

use std::sync::{Arc, OnceLock};

use anyhow::{Context as _, Result, anyhow};

use super::super::GpuRenderer;

struct DeviceLoss {
    reason: wgpu::DeviceLostReason,
    message: String,
}

pub(in crate::gpu) struct DeviceState {
    loss: Arc<OnceLock<DeviceLoss>>,
}

impl DeviceState {
    pub(super) fn register(device: &wgpu::Device) -> Self {
        let loss = Arc::new(OnceLock::new());
        let callback_loss = Arc::clone(&loss);
        device.set_device_lost_callback(move |reason, message| {
            let _ = callback_loss.set(DeviceLoss { reason, message });
        });
        Self { loss }
    }

    pub(super) fn check(&self) -> Result<()> {
        if let Some(loss) = self.loss.get() {
            if loss.message.is_empty() {
                return Err(anyhow!("wgpu device lost ({:?})", loss.reason));
            }
            return Err(anyhow!(
                "wgpu device lost ({:?}): {}",
                loss.reason,
                loss.message
            ));
        }
        Ok(())
    }
}

impl GpuRenderer {
    /// Deliver pending driver callbacks without waiting for GPU completion.
    /// Known loss must stop before Lua advances or GPU resources are touched.
    pub(crate) fn check_device(&self) -> Result<()> {
        self.device_state.check()?;
        self.device
            .poll(wgpu::PollType::Poll)
            .context("poll Stella GPU device")?;
        self.device_state.check()
    }
}
