//! Prepared-texture synchronization and texture-pair bind groups.

use super::super::resources::{create_capture_texture, upload_texture};
use super::super::*;

impl GpuRenderer {
    pub(super) fn sync_textures(
        &mut self,
        assets: &AssetCatalog,
        frame: &PreparedFrame,
    ) -> Result<()> {
        self.retired_textures
            .extend(frame.retired_textures.iter().cloned());
        self.retire_unused_textures(frame, false);
        for operation in &frame.operations {
            let PreparedOperation::Capture(name) = operation else {
                continue;
            };
            if !self.textures.contains_key(name) {
                self.textures.insert(
                    name.clone(),
                    create_capture_texture(&self.device, name, self.resolution),
                );
            }
        }
        for name in &frame.required_textures {
            if self.textures.contains_key(name) {
                continue;
            }
            let image = frame
                .transient_textures
                .get(name)
                .map(AsRef::as_ref)
                .or_else(|| assets.textures.get(name))
                .ok_or_else(|| anyhow!("prepared GPU texture is missing from cache: {name}"))?;
            self.textures.insert(
                name.clone(),
                upload_texture(&self.device, &self.queue, name, &image.image),
            );
        }
        Ok(())
    }

    pub(super) fn retire_unused_textures(&mut self, frame: &PreparedFrame, completed: bool) {
        let releasable = self
            .retired_textures
            .iter()
            .filter(|name| {
                !frame.required_textures.contains(*name)
                    && (completed || !frame.operations.iter().any(|operation| {
                        matches!(operation, PreparedOperation::Capture(target) if target == *name)
                    }))
            })
            .cloned()
            .collect::<Vec<_>>();
        for name in releasable {
            self.textures.remove(&name);
            self.texture_bind_groups
                .retain(|(base, fill), _| base != &name && fill != &name);
            self.retired_textures.remove(&name);
        }
    }

    pub(super) fn texture_bind_group(&mut self, base: &str, fill: &str) -> Result<wgpu::BindGroup> {
        let key = (base.to_owned(), fill.to_owned());
        if let Some(bind_group) = self.texture_bind_groups.get(&key) {
            return Ok(bind_group.clone());
        }
        let base_view = &self
            .textures
            .get(base)
            .ok_or_else(|| anyhow!("GPU base texture is missing: {base}"))?
            .view;
        let fill_view = &self
            .textures
            .get(fill)
            .ok_or_else(|| anyhow!("GPU fill texture is missing: {fill}"))?
            .view;
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Stella sprite texture pair"),
            layout: &self.sprite_texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(base_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.base_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(fill_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.fill_sampler),
                },
            ],
        });
        self.texture_bind_groups.insert(key, bind_group.clone());
        Ok(bind_group)
    }
}
