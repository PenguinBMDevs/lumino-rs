# 内置字体来源与授权

## 文件

| 文件 | 内容 | 大小 |
|---|---|---|
| `NotoSansSC-Regular.ttf` | Noto Sans SC，wght 固定 400，GB2312 子集 | 约 2.2 MB |
| `NotoSansSC-Bold.ttf` | Noto Sans SC，wght 固定 700，GB2312 子集 | 约 2.2 MB |
| `LICENSE-OFL-1.1.txt` | SIL Open Font License 1.1 全文（含上游版权声明） | — |

## 来源

- 上游：Noto Sans SC Variable（`NotoSansSC-VF.ttf`），版权
  `© 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'.`
- 授权：SIL Open Font License 1.1（见 `LICENSE-OFL-1.1.txt`）。
- 取自体：Windows 系统字体目录中的 `NotoSansSC-VF.ttf`（同一字体，未改动字形）。

## 做过什么处理

1. **按字重实例化**：上游是可变字体，且 `fvar` 的 `wght` **默认值是 100（Thin）**。
   cosmic-text 只按家族名 + 字重挑 face，不会主动设置可变轴，直接用 VF 会全站渲染成
   极细字重。故用 `fonttools.varLib.instancer` 固定 `wght=400` / `wght=700` 两个静态实例。
2. **子集化**：`fonttools.subset` 保留
   - GB2312 全字符集（约 6763 汉字，覆盖简体中文常用字）
   - ASCII + Latin-1
   - 通用标点（U+2000–U+206F）、CJK 标点（U+3000–U+303F）、全角形式（U+FF00–U+FFEF）
   去掉 hinting 与 DSIG；结果每档约 2.2 MB。

   字库外的生僻字依赖系统字体回退：fork 的 `graphics/src/text.rs` 在 android 上显式
   `load_fonts_dir("/system/fonts")`，桌面由 cosmic-text 自行扫描系统字体。
3. 重写 `name` 表把家族统一成 `Noto Sans SC`（Regular/Bold 同族），并按字重设置
   `OS/2.usWeightClass` 与 `head.macStyle`，使 `Font { family: "Noto Sans SC", weight: Bold }`
   能命中 Bold 档。

## 合规性

OFL-1.1 要求「修改版本不得使用保留字体名（Reserved Font Name）」。上游保留名只有
`Source`，本子集使用的家族名是 `Noto Sans SC`（不含 `Source`），**不构成冲突**。
授权全文与版权声明随字体一同分发（`LICENSE-OFL-1.1.txt`）。
