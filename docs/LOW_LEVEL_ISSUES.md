# 低级错误问题汇总

本文档记录了在 `feat/midieditor` 分支代码审查中发现的低级错误。

---

## 一、Mutex 毒化问题 (高危)

### 位置
- 文件: `crates/core/src/midi.rs:103`
- 代码: `.map(|mgr| mgr.lock().unwrap().stats())`

### 问题描述
如果 `Mutex` 发生毒化（getter 代码中发生 panic），`lock().unwrap()` 会直接 panic，导致整个应用崩溃。

### 建议修复
```rust
// 方法1: 处理毒化错误，返回默认统计
.lock()
.map(|guard| guard.stats())
.unwrap_or_else(|poisoned| {
    let guard = poisoned.into_inner();
    guard.stats()
})

// 方法2: 使用 try_lock 避免阻塞
if let Ok(guard) = mgr.try_lock() {
    guard.stats()
} else {
    // 默认统计
}
```

### TODO 标记
已在 `crates/core/src/midi.rs:103` 添加 TODO 注释。

---

## 二、HashMap 查找冗余 (代码质量)

### 位置
- 文件: `crates/core/src/midi/managed_midi.rs`
- 代码位置: `get_track_events` 函数

### 问题描述
```rust
// 冗余的 HashMap 查找
if self.in_memory_tracks.contains_key(&track_index) {
    return Ok(self.in_memory_tracks.get(&track_index).unwrap());
}
```

代码先调用 `contains_key()` 检查，然后又调用 `get()` 查找，导致两次哈希计算。

### 建议修复
```rust
// 优化: 直接使用 get().map_or 避免重复查找
if let Some(events) = self.in_memory_tracks.get(&track_index) {
    return Ok(events);
}
```

### TODO 标记
已在 `crates/core/src/midi/managed_midi.rs:471` 添加 TODO 注释。

---

## 三、LRU 缓存同步问题 (潜在风险)

### 位置
- 文件: `crates/core/src/midi/managed_midi.rs`

### 问题描述
在 `evict_oldest_loaded()` 中删除 `loaded_tracks` 条目时，如果 `lru_order` 同步失败，可能导致状态不一致。

### 建议修复
- 检查 `evict_oldest_loaded()` 实现，确保原子更新
- 考虑使用 `HashMap` 的 `remove()` 返回值做双重检查

### TODO 标记
已在 `crates/core/src/midi/managed_midi.rs:498` 添加 TODO 注释。

---

## 四、Platform 特定 Panic (低危)

### 位置
- 文件: `src/platform/windows.rs`
- 代码: `panic!("Not a Windows window")`

### 问题描述
当在非 Windows 平台调用 Windows 特定函数时会 panic。

### 建议修复
在调用处添加编译期条件：
```rust
#[cfg(target_os = "windows")]
pub fn setup_resize_border(window: &Window) { ... }
```

或者返回 `Result` 类型，让调用方处理错误。

### TODO 标记
已在 `src/platform/windows.rs:88` 添加 TODO 注释。

---

## 五、代码可读性问题 (低危)

### 位置
- 文件: `crates/ui/src/editor.rs`
- 代码: `if notes.is_none() || notes.unwrap().is_empty()`

### 问题描述
虽然 `||` 是短路运算符，但这种写法容易误导，包含不必要的 `notes.unwrap()` 调用。

### 建议修复
```rust
// 修复方案1: 使用 match
match self.track_notes.get(&track_idx) {
    Some(notes) if !notes.is_empty() => { /* 处理 */ }
    _ => return Vec::new(),
}

// 修复方案2: 使用 if let
if let Some(notes) = self.track_notes.get(&track_idx) {
    if !notes.is_empty() {
        // 处理
        return Vec::new();
    }
}
return Vec::new();
```

### TODO 标记
已在 `crates/ui/src/editor.rs:284` 添加 TODO 注释。

---

## 问题汇总表

| 优先级 | 问题 | 文件 | 严重程度 | 状态 |
|--------|------|------|----------|------|
| 高 | Mutex 毒化 | `crates/core/src/midi.rs` | 可能崩溃 | TODO 已添加 |
| 中 | HashMap 冗余 | `crates/core/src/midi/managed_midi.rs` | 性能/可读性 | TODO 已添加 |
| 中 | LRU 同步 | `crates/core/src/midi/managed_midi.rs` | 潜在数据不一致 | TODO 已添加 |
| 低 | Platform panic | `src/platform/windows.rs` | 跨平台兼容性 | TODO 已添加 |
| 低 | 代码可读性 | `crates/ui/src/editor.rs` | 维护性 | TODO 已添加 |

---

## 后续行动

1. 优先修复 Mutex 毒化问题
2. 优化 HashMap 查找逻辑
3. 检查 LRU 缓存实现的原子性
4. 添加跨平台编译期检查
5. 重构代码可读性问题

---

*本文档创建时间: 2026-03-10*
