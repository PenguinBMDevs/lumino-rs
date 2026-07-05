#[cfg(test)]
mod tests {
    use super::super::core_impl::HiResRenderer;
    use crate::config::HiResConfig;
    use crate::types::TileCoord;

    /// 尝试创建 wgpu 设备；无可用 GPU 时返回 None，测试自动跳过。
    fn try_create_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter =
            futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
            .ok()?;
        let (device, queue) =
            futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            }))
            .ok()?;
        Some((device, queue))
    }

    #[test]
    fn test_dirty_overlay_replaces_base_tile_state() {
        let Some((device, queue)) = try_create_device() else {
            // 当前环境无可用 GPU，跳过 GPU 相关断言
            return;
        };

        let config = HiResConfig::default();
        let mut renderer = HiResRenderer::new(&device, config, wgpu::TextureFormat::Rgba8UnormSrgb);
        let coord = TileCoord::new(0, 0);
        let pixels = vec![128u8; 64 * 64 * 4];

        // 上传基础贴图：应存在基础贴图，无覆层
        renderer.upload_tile(&device, &queue, coord, &pixels, 64, 64);
        assert!(renderer.has_tile(&coord));
        assert!(!renderer.has_dirty_overlay(&coord));

        // 上传脏区域覆层：基础贴图仍在，但 render 会跳过它，由覆层替代
        renderer.upload_dirty_overlay(&device, &queue, coord, &pixels, 64, 64);
        assert!(renderer.has_tile(&coord));
        assert!(renderer.has_dirty_overlay(&coord));

        // 新的基础贴图上传后，脏区域覆层被清理（模拟冷静期后重生成）
        renderer.upload_tile(&device, &queue, coord, &pixels, 64, 64);
        assert!(renderer.has_tile(&coord));
        assert!(!renderer.has_dirty_overlay(&coord));
    }
}
