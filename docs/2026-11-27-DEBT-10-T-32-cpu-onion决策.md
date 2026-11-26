# DEBT-10 / T-32：cpu-onion 双份代码决策

> 日期：2026-11-27
> 状态：决策已定，迁移待排期
> 关联：`2026-11-26-DEBT-10-结构减法与尺寸治理.md`

## 1. 事实

| 项 | `crates/editor/cpu-onion` | `examples/cpu-onionskin` |
|---|---|---|
| 角色 | 生产共享 crate（`lumino-cpu-onion`） | 独立示例程序 |
| 依赖方 | `lumino-gfx`、`lumino-ui`、`cpu_gpu_parity` 测试 | 仅自身 |
| 测试 | `region_render` / `palette_refresh` / `overview_lod` / `zoom_bench` / `perf_1080p` | `edit/tests.rs` 等示例内测试 |
| 同源文件 | `camera.rs` 192 行、`simd.rs` 321 行、`color.rs` 170 行… | `camera.rs` 276 行、`simd.rs` 286 行、`color.rs` 223 行… |
| 漂移证据 | — | `camera.rs` 两版 diff 340 行 |

结论：示例里保留的是一份**已分叉的旧实现**，不是「示例专用简化版」。

## 2. 决策

1. **共享内核 = `crates/editor/cpu-onion`**。生产、基准、对拍测试都以它为准。
2. 示例只保留交互层：`app/*`（窗口 / 事件 / present）、`edit/*`（选择 / 拖拽 / 历史）、
   `text.rs`、`profiling.rs`。
3. 示例的 `camera.rs`、`simd.rs`、`render/*`、`color.rs`、`band.rs` 必须删除并改为
   `lumino-cpu-onion` 的公开 API；删除前先做像素对拍（同一相机参数、同一轨道集合）。
4. 不允许「示例需要什么就在示例里改一份」；示例需要的新能力先加到共享 crate，
   由 crate 的测试守护。

## 3. 迁移步骤（后续卡）

1. 给共享 crate 补示例需要的入口（若缺）：`CanvasCamera` 构造、`DensityPyramid` 参数、
   调色板注入。
2. 示例逐文件替换并删除分叉文件；每一步用 `cargo run -p cpu-onionskin` 截图对照。
3. 删除 `examples/cpu-onionskin/src/{camera,simd,color,band,render}` 后，
   `cargo check --workspace` + 示例自测通过。
4. 在示例 README 写明「渲染内核来自 lumino-cpu-onion」。

## 4. 验收

- `examples/cpu-onionskin` 不再包含与共享 crate 同名的渲染文件；
- `rg "fn to_oklch|struct CanvasCamera" examples/` 为空；
- 示例与生产在同一相机参数下的输出像素差 ≤ 现有 `cpu_gpu_parity` 阈值。

## 5. API 差异清单（2026-11-27 复核，迁移前必读）

| 模块 | 示例 | crate | 迁移结论 |
|---|---|---|---|
| `color` | `pack_rgb`（0x00RRGGBB）、`linear_to_pixel`、`blend_*` | `pack_argb`、`linear_to_argb`、无 blend | **已迁移常量与 sRGB**；其余语义不同，不能直接换 |
| `simd` | — | — | ✅ 已完成：crate `pub mod simd`、统一 `masked_fill_row(..., count_coverage)`、示例 re-export |
| `camera` | `Camera`（276 行） | `CanvasCamera`（192 行） | 需逐方法对照 `screen<->canvas`、滚动/缩放语义后重写调用点 |
| `render/band` | `LodMode`、`BandBuffer` | `CpuOnionRenderer::render_strip` / `shift_frame` | 示例的 band 缓冲是旧 LOD 实现，需按 crate 的 strip 接口重写 |
| `render/buffer` | `BandBuffer` | `frame` + renderer 内部缓冲 | 无公开等价物 |
| `render/density/*` | 示例自己的 grow/coverage/render | `density::{build,grow,coverage,render}` | 需逐函数对照 |

结论：T-32b 不是「删文件 + re-export」，而是**示例渲染层移植**；必须逐模块做像素对拍。
需人类决定：移植示例，还是把示例降级为「仅演示 app/edit 层、渲染全部走 crate」并接受
示例原有 LOD 行为变化。
