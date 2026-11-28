use std::sync::Arc;

use wgpu;
use wgpu::util::DeviceExt;

use super::super::key_layout;
use super::instances::{build_instances, build_uniforms, instances_to_bytes};
use super::{
    KEY_HEIGHT_RATIO, KeyboardColors, KeyboardRenderer, MAX_KEY_HEIGHT, MIN_KEY_HEIGHT,
    WORKGROUP_SIZE, ZERO_KEYCOLORS,
};

/// 场景尺寸与相机参数：收敛 `render_scene` 的入参，逐阶段传给私有方法。
///
/// 不新增对外 API，仅在本模块内流转；字段顺序即离屏渲染的依赖顺序。
#[derive(Debug)]
struct SceneParams {
    width: u32,
    height: u32,
    key_count: u32,
    count: u32,
    zoom_x: f32,
    scroll_x: f32,
    current_track: u32,
    kb_h: f32,
    keyboard_y: f32,
}

impl KeyboardRenderer {
    /// 渲染「下落式音符 + 底部键盘」到离屏纹理，返回其纹理视图供 上一代 `shader` 图元直接采样合成。
    ///
    /// 返回 `Some(Arc<TextureView>)` 表示本次渲染完成（调用方据此更新面板持有的视图）；
    /// 返回 `None` 表示离屏资源尚未就绪（尺寸/缓冲尚未分配），下一帧重试即可。
    ///
    /// 注意：**不做 CPU 读回**。纹理由 上一代 在自身渲染通道内直接采样，GPU→GPU 合成，
    /// 与钢琴卷帘洋葱皮同一路径，因此不进 `image::Handle`、不进 上一代 图集、不闪烁。
    ///
    /// - `note_data`：渲染线程发布的活体 GPU 实例缓冲与实例数；`None` 时仅渲染键盘。
    /// - `zoom_x` / `scroll_x`：与钢琴卷帘 X 缩放/滚动一致，驱动音符落点与时间流。
    /// - `current_track`：主音轨编码（`current_track_idx + 1`），用于蓝色覆盖。
    #[allow(clippy::too_many_arguments)]
    pub fn render_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        key_count: u32,
        note_data: Option<(wgpu::Buffer, u32)>,
        zoom_x: f32,
        scroll_x: f32,
        current_track: u32,
    ) -> Option<Arc<wgpu::TextureView>> {
        puffin::profile_scope!("kbrd_render_scene");
        let width = width.max(1);
        let height = height.max(1);
        // 键盘底条：高度按宽度比例联动，贴底
        let kb_h = (width as f32 * KEY_HEIGHT_RATIO).clamp(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
        let p = SceneParams {
            width,
            height,
            key_count,
            count: note_data.as_ref().map(|(_, c)| *c).unwrap_or(0),
            zoom_x,
            scroll_x,
            current_track,
            kb_h,
            keyboard_y: height as f32 - kb_h,
        };

        // 资源准备阶段：扩容 + 清零 + 每帧 uniform/间接参数上传
        let key_instance_count = self.prepare_resources(device, queue, &p)?;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        // 两个 compute pass 严格先于 render pass，顺序即语义
        self.dispatch_cull(device, &mut encoder, note_data.as_ref());
        self.dispatch_keycolor(device, &mut encoder, note_data.as_ref());

        let tex_view = self.tex_view.as_ref()?;
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("piano_waterfall_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: tex_view.as_ref(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 透明清屏：不填白，瀑布流区域透出面板背景
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 绘制顺序：音符 → 键盘，键盘覆盖落点线
            self.draw_notes(device, &mut rp, note_data.as_ref(), p.count);
            self.draw_keyboard(device, &mut rp, key_instance_count);
        }

        queue.submit(std::iter::once(encoder.finish()));

        // 不做 CPU 读回：直接返回离屏纹理视图，由 上一代 `shader` 图元在自身渲染通道内采样合成
        // （GPU→GPU）。返回克隆的 `Arc`，即使后续纹理重建，旧视图仍可被在途图元安全引用。
        self.tex_view.clone()
    }

    /// 准备离屏资源与每帧上传：纹理/缓冲扩容、活跃键颜色清零、键位实例缓冲、
    /// 音符 uniform 与间接绘制参数。返回键盘实例数供后续绘制阶段使用。
    fn prepare_resources(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        p: &SceneParams,
    ) -> Option<u32> {
        self.ensure_targets(device, p.width, p.height);
        self.ensure_visible_indices(device, p.count);
        self.ensure_key_colors(device);
        // `ensure_targets` 刚保证分配；理论不可达，防御性跳过本帧
        self.tex_view.as_ref()?;

        // 活跃键颜色缓冲：每帧先清零，再由 keycolor compute 写入“正落键”的键
        let Some(key_colors_buf) = self.key_colors.as_ref() else {
            // `ensure_key_colors` 刚保证分配；理论不可达，防御性跳过本帧
            return None;
        };
        queue.write_buffer(key_colors_buf, 0, &ZERO_KEYCOLORS);

        let colors = KeyboardColors::pure();
        let mut keys = key_layout::build_layout(p.width as f32, p.kb_h, p.key_count);
        keys.sort_by_key(|k| k.is_black); // 白键在前、黑键在后，黑键覆盖白键
        let instances = build_instances(p.width, p.height as f32, p.keyboard_y, &keys, &colors);

        // 复用持久化实例缓冲：键盘几何仅依赖 (width, height, key_count)，
        // 与滚动/缩放/播放进度无关，逐帧重建 + drop 会触发驱动延迟释放，
        // 在播放自动滚动时造成突发性尖刺。仅当三者变化时才新建。
        if self.instance_buffer.is_none()
            || self.inst_w != p.width
            || self.inst_h != p.height
            || self.inst_keys != p.key_count
        {
            let instance_bytes = instances_to_bytes(&instances);
            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("piano_waterfall_keyboard_instances"),
                contents: &instance_bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.instance_buffer = Some(buf);
            self.inst_w = p.width;
            self.inst_h = p.height;
            self.inst_keys = p.key_count;
        }
        // 上方刚保证分配；理论不可达，防御性跳过本帧
        self.instance_buffer.as_ref()?;

        // 音符 uniform（含落点线 keyboard_y）
        let uni = build_uniforms(
            p.width as f32,
            p.height as f32,
            p.zoom_x,
            p.scroll_x,
            p.current_track,
            p.key_count,
            p.keyboard_y,
        );
        queue.write_buffer(&self.uniform_buffer, 0, &uni);

        // 间接绘制参数：vertex_count=6，instance_count 由 compute 原子自增
        let draw_args_bytes = {
            let mut db = Vec::with_capacity(16);
            for u in [6u32, 0u32, 0u32, 0u32] {
                db.extend_from_slice(&u.to_le_bytes());
            }
            db
        };
        queue.write_buffer(&self.draw_args, 0, &draw_args_bytes);

        Some(instances.len() as u32)
    }

    /// 可视区间剔除 compute：把可见音符索引写入 visible_indices + draw_args[1]。
    fn dispatch_cull(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        note_data: Option<&(wgpu::Buffer, u32)>,
    ) {
        // 可视区间剔除：仅把可见音符索引写入 visible_indices + draw_args[1]
        // 超量音符（>65535 工作群组）分块调度，每块带各自 dispatch 偏移，
        // 仍累加到同一 draw_args 原子计数，主绘制只画可见音符。
        if let Some((buf, c)) = note_data
            && *c > 0
        {
            let total_wg = (*c).div_ceil(WORKGROUP_SIZE);
            let max_wg = 65535u32;
            let mut cp = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("piano_waterfall_cull"),
                timestamp_writes: None,
            });
            cp.set_pipeline(&self.cull_pipeline);
            let mut dispatched = 0u32;
            while dispatched < total_wg {
                let wg_count = (total_wg - dispatched).min(max_wg);
                let offset = dispatched * WORKGROUP_SIZE;
                let mut ob = Vec::with_capacity(16);
                ob.extend_from_slice(&offset.to_le_bytes());
                ob.extend_from_slice(&[0u8; 12]);
                let off_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("piano_waterfall_cull_offset"),
                    contents: &ob,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let cull_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("piano_waterfall_cull_bg"),
                    layout: &self.cull_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(buf.as_entire_buffer_binding()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(
                                self.uniform_buffer.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(
                                self.visible_indices.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Buffer(
                                self.draw_args.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Buffer(
                                off_buf.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                });
                cp.set_bind_group(0, &cull_bg, &[]);
                cp.dispatch_workgroups(wg_count, 1, 1);
                dispatched += wg_count;
            }
        }
    }

    /// 活跃键颜色 compute：把落键高亮写入 key_colors。
    fn dispatch_keycolor(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        note_data: Option<&(wgpu::Buffer, u32)>,
    ) {
        // 活跃键颜色 compute：把“正跨过键盘线（落键）”的音符对应键颜色写入 key_colors。
        // 与剔除同分块策略（>65535 工作群组分块），复用同一条 notes 缓冲与 uniform。
        if let Some((buf, c)) = note_data
            && *c > 0
        {
            // `prepare_resources` 已保证分配；理论不可达，防御性跳过本 pass
            let Some(key_colors_buf) = self.key_colors.as_ref() else {
                return;
            };
            let total_wg = (*c).div_ceil(WORKGROUP_SIZE);
            let max_wg = 65535u32;
            let mut cp2 = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("piano_waterfall_keycolor"),
                timestamp_writes: None,
            });
            cp2.set_pipeline(&self.keycolor_pipeline);
            let mut dispatched = 0u32;
            while dispatched < total_wg {
                let wg_count = (total_wg - dispatched).min(max_wg);
                let offset = dispatched * WORKGROUP_SIZE;
                let mut ob = Vec::with_capacity(16);
                ob.extend_from_slice(&offset.to_le_bytes());
                ob.extend_from_slice(&[0u8; 12]);
                let off_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("piano_waterfall_keycolor_offset"),
                    contents: &ob,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let kc_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("piano_waterfall_keycolor_bg"),
                    layout: &self.keycolor_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(buf.as_entire_buffer_binding()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(
                                self.uniform_buffer.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(
                                key_colors_buf.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Buffer(
                                off_buf.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                });
                cp2.set_bind_group(0, &kc_bg, &[]);
                cp2.dispatch_workgroups(wg_count, 1, 1);
                dispatched += wg_count;
            }
        }
    }

    /// 绘制下落式音符：仅可见音符，间接绘制，复用渲染线程活体 GPU 实例缓冲。
    fn draw_notes(
        &self,
        device: &wgpu::Device,
        rp: &mut wgpu::RenderPass<'_>,
        note_data: Option<&(wgpu::Buffer, u32)>,
        count: u32,
    ) {
        // 1) 下落式音符（仅可见音符，间接绘制；复用渲染线程活体 GPU 实例缓冲）
        if let Some((buf, _)) = note_data
            && count > 0
        {
            let bg_note = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("piano_waterfall_note_bg"),
                layout: &self.note_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(buf.as_entire_buffer_binding()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(
                            self.uniform_buffer.as_entire_buffer_binding(),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(
                            self.visible_indices.as_entire_buffer_binding(),
                        ),
                    },
                ],
            });
            rp.set_pipeline(&self.note_pipeline);
            rp.set_bind_group(0, &bg_note, &[]);
            rp.draw_indirect(&self.draw_args, 0);
        }
    }

    /// 绘制底部钢琴键盘：绑定活跃键颜色缓冲，覆盖在落点线处。
    fn draw_keyboard(
        &self,
        device: &wgpu::Device,
        rp: &mut wgpu::RenderPass<'_>,
        key_instance_count: u32,
    ) {
        // 2) 底部钢琴键盘（覆盖在落点线处，确保键位清晰）
        // 绑定活跃键颜色缓冲：键盘着色器据此混合“落键”高亮（复用卷帘瀑布流配色）
        let Some(key_colors_buf) = self.key_colors.as_ref() else {
            // `prepare_resources` 已保证分配；理论不可达，防御性跳过本层
            return;
        };
        let Some(instance_buffer) = self.instance_buffer.as_ref() else {
            // 上方刚保证分配；理论不可达，防御性跳过本层
            return;
        };
        let bg_key = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("piano_waterfall_key_bg"),
            layout: &self.key_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(key_colors_buf.as_entire_buffer_binding()),
            }],
        });
        rp.set_pipeline(&self.pipeline);
        rp.set_bind_group(0, &bg_key, &[]);
        rp.set_vertex_buffer(0, self.quad_buffer.slice(..));
        rp.set_vertex_buffer(1, instance_buffer.slice(..));
        if key_instance_count > 0 {
            rp.draw(0..6, 0..key_instance_count);
        }
    }
}
