use lumino_gfx::midiconsole_renderer::{CellGpu, MidiconsoleGpuContext, pack_rgb};

use super::cpu::{MidiConsoleFrameArgs, render_midicomsole_frame};
use super::*;

/// 与 [`render_midicomsole_frame`] 同签名的 GPU 加速版本。
///
/// 复用 CPU 廉价网格构建（状态机逻辑，不慢），仅把昂贵的 ab_glyph 逐帧描边与
/// CRT 逐像素后处理搬上 GPU（`lumino_gfx::midiconsole_renderer`）。
/// 任意 GPU 初始化/渲染失败都安全回退到 CPU 路径，保证导出不中断。
pub fn render_midicomsole_frame_gpu(args: MidiConsoleFrameArgs<'_>) {
    let MidiConsoleFrameArgs {
        renderer,
        frame,
        frame_width,
        frame_height,
        document,
        tick,
        ppq,
        fps,
    } = args;
    let gw = COLS as usize;
    let gh = ROWS as usize;
    let fw = frame_width as usize;
    let fh = frame_height as usize;
    let out_len = fw * fh * 4;
    if frame.len() < out_len {
        return;
    }

    // 1) 复用 CPU 网格构建（廉价）
    let mut grid = vec![Cell::blank(); gw * gh];
    renderer.render(&mut grid, document, tick, ppq, fps);

    // 2) 转换为 GPU 单元（0xRRGGBB）
    let mut cells: Vec<CellGpu> = Vec::with_capacity(gw * gh);
    for c in &grid {
        cells.push(CellGpu {
            ch: c.ch as u32,
            fg: pack_rgb(c.fg[0], c.fg[1], c.fg[2]),
            bg: pack_rgb(c.bg[0], c.bg[1], c.bg[2]),
            _pad: 0,
        });
    }

    // 3) GPU 渲染（线程内缓存设备/渲染器，失败回退 CPU）
    let rgba = GPU_CTX.with(|g| {
        if GPU_DISABLED.with(|d| *d.borrow()) {
            return None;
        }
        let mut guard = g.borrow_mut();
        let need_rebuild = guard
            .as_ref()
            .is_none_or(|c| c.width != frame_width || c.height != frame_height);
        if need_rebuild {
            match build_gpu_ctx(frame_width, frame_height) {
                Some(ctx) => *guard = Some(ctx),
                None => {
                    GPU_DISABLED.with(|d| *d.borrow_mut() = true);
                    return None;
                }
            }
        }
        let ctx = guard.as_mut().expect("GPU 上下文已建立");
        Some(ctx.ctx.render_frame(&cells, tick))
    });

    match rgba {
        Some(rgba) => {
            // RGBA → BGRA（导出编码器按 "bgra" 消费 MidiConsole 帧）
            for y in 0..fh {
                for x in 0..fw {
                    let si = (y * fw + x) * 4;
                    let di = si;
                    frame[di] = rgba[si + 2];
                    frame[di + 1] = rgba[si + 1];
                    frame[di + 2] = rgba[si];
                    frame[di + 3] = 255;
                }
            }
        }
        None => render_midicomsole_frame(MidiConsoleFrameArgs {
            renderer,
            frame,
            frame_width,
            frame_height,
            document,
            tick,
            ppq,
            fps,
        }),
    }
}

/// 线程内缓存的 GPU 上下文（来自 `lumino_gfx`，按分辨率缓存）
struct GpuCtx {
    width: u32,
    height: u32,
    ctx: MidiconsoleGpuContext,
}

thread_local! {
    /// 当前线程的 GPU 上下文（导出在独立后台线程运行，单例缓存即可）
    static GPU_CTX: std::cell::RefCell<Option<GpuCtx>> = const { std::cell::RefCell::new(None) };
    /// GPU 不可用时置位，避免每帧重复尝试创建适配器
    static GPU_DISABLED: std::cell::RefCell<bool> = const { std::cell::RefCell::new(false) };
}

/// 创建 GPU 上下文（无可用适配器时返回 `None`）
fn build_gpu_ctx(width: u32, height: u32) -> Option<GpuCtx> {
    let ctx = MidiconsoleGpuContext::new(width, height)?;
    Some(GpuCtx { width, height, ctx })
}
