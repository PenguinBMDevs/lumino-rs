## 变更摘要

<!-- 一两句话说清楚：改了什么、为什么改。 -->

## 变更类型

- [ ] Bug 修复
- [ ] 新功能
- [ ] 性能优化（**必须填下面的性能对照表**）
- [ ] 重构 / 工程卫生（行为不变）
- [ ] 文档 / 构建 / CI

## 验证

- [ ] `cargo fmt`（或改动文件已 rustfmt）
- [ ] `cargo clippy` 零新增警告
- [ ] 相关 crate 单测通过（`cargo test -p <crate>`）
- [ ] 涉及音频输出：同块长/同配置逐位一致（贴 FNV/golden 哈希）

## 性能对照表（性能类 PR 必填）

> 口径见 `docs/2026-10-31-REF-LGS-013-性能护栏与A-B方法.md`：
> **双二进制 + 顺序交替 + ≥6 对 + 中位数 + 记录 GPU 时钟/温度**；
> 单次对比不作为验收证据。附上 `lgs_ab.py` 产出的 JSON/CSV。

| 用例（MIDI/块长/通道） | baseline | after | delta | 对数 | 判定 |
|---|---|---:|---:|---:|---|
| `sustained65536 @512` |  |  |  | 6 |  |
| `sustained65536 @2048` |  |  |  | 6 |  |
| `multiport_16p_256ch @512` |  |  |  | 6 |  |

命令：

```bash
python scripts/lgs_ab.py \
  --baseline <baseline.exe> --after <after.exe> \
  --pairs 6 --gate-pct 2 --label <card> \
  --json bench-results/ab.json --csv bench-results/ab.csv
```

- 判定阈值：本地精测 2%，CI 10%；超过阈值必须在描述里解释。
- 采样内存/显存变化：GMEM/CMEM（如涉及音源加载路径）。

## 关联

- 任务卡 / Issue：
- 依赖 / 被依赖：
- 回滚方式：

## 风险与说明

<!-- 已知边界、未覆盖场景、需要 reviewer 重点看的地方。 -->
