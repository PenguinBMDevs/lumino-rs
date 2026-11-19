# Lumino 测试指南

本文档提供 Lumino 项目中所有测试的快速参考指南。

## 测试概述

Lumino 项目包含以下类型的测试：

| 类别 | 测试文件 | 描述 |
|------|---------|------|
| **协作功能测试** | `tests/collaboration_ui_test.rs` | 多人协作功能完整测试套件（默认 `#[ignore]`，需外部协作服务器） |
| **协作基础测试** | `tests/collaboration_full_test.rs` | 协作基础连接测试 + 本地序列化单测 |
| **本地协作 ID 同步测试** | `tests/collab_id_sync_local.rs` | 真实 u64 音符 ID 跨客户端同步（默认 `#[ignore]`，需本地协作服务器） |
| **公共测试辅助** | `tests/common.rs` | 测试共享辅助（事件收集器等，非独立测试） |

---

## 快速开始



> 统一使用 `cargo nextest`（禁止 `cargo test`）；未安装先执行
> `cargo install cargo-nextest --locked`。详见 `AGENTS.md` 的 Test Commands。

### 运行所有测试

```bash
cargo nextest run --workspace
```

### 运行特定测试

```bash
# 协作 UI 测试套件（默认 #[ignore]，必须加 --run-ignored only；需外部协作服务器）
cargo nextest run --test collaboration_ui_test test_collaboration_ui_all --run-ignored only --no-capture

# 协作基础测试（默认 #[ignore] 的主测试；其中的本地序列化单测无需 --run-ignored）
cargo nextest run --test collaboration_full_test --run-ignored only --no-capture

# 本地协作 ID 同步测试（默认 #[ignore]；需本地 lumino-server-rs，端口 3000）
cargo nextest run --test collab_id_sync_local --run-ignored only --no-capture
```

---

## 协作功能测试详解

### 测试文件: `tests/collaboration_ui_test.rs`

这是主要的协作功能测试套件，测试多人协作的核心功能。

#### 主测试函数

```rust
#[tokio::test]
async fn test_collaboration_ui_all()
```

此函数顺序执行以下三个子测试：

#### 子测试 1: 鼠标指针同步

**函数**: `test_mouse_cursor_sync_internal()`

**测试内容**:
- ✓ 客户端A创建房间
- ✓ 客户端B加入房间
- ✓ A→B 鼠标指针投影可见
- ✓ A→B 音符同步
- ✓ B→A 鼠标指针投影可见
- ✓ B→A 音符同步

**关键验证点**:
- 鼠标位置坐标准确性
- 音符属性（tick, key, velocity）
- 用户ID匹配

#### 子测试 2: 音符批量同步

**函数**: `test_note_batch_sync_internal()`

**测试内容**:
- ✓ 批量创建 3 个音符
- ✓ 一次性发送批量操作
- ✓ 客户端B接收全部音符

**音符数据**:
```rust
Note { tick: 0.0,   key: 60, ... }
Note { tick: 480.0, key: 64, ... }
Note { tick: 960.0, key: 67, ... }
```

#### 子测试 3: 鼠标指针连续移动

**函数**: `test_mouse_movement_sync_internal()`

**测试内容**:
- ✓ 连续发送 5 个鼠标位置
- ✓ 每个位置间隔 100ms
- ✓ 验证实时同步性能

**位置序列**:
```rust
(100, 100) → (200, 150) → (300, 200) → (400, 250) → (500, 300)
```

---

## 测试实现技术

### 事件收集器模式

所有协作测试使用事件收集器来捕获异步事件：

```rust
let collector = EventCollector::new();
let mut client = CollaborationClient::new(config);
client.set_event_callback(collector.callback());
```

### 等待特定事件

```rust
// 等待认证事件
let user_id = collector
    .wait_for(|e| {
        if let CollaborationEvent::Authenticated { user_id, .. } = e {
            Some(user_id.clone())
        } else {
            None
        }
    }, 5000) // 5秒超时
    .await
    .ok_or("认证超时")?;
```

### 检查事件存在

```rust
// 检查是否收到鼠标更新
let received = collector
    .contains_event(|e| {
        matches!(e, CollaborationEvent::MouseUpdate { user_id, .. } if user_id == expected_id)
    }, 5000)
    .await;
```

---

## 配置信息

### 服务器配置

```rust
ClientConfig {
    server_host: "lumino-02.afeu20u3jfocas.dpdns.org".to_string(),
    server_port: 80,
    username: "测试用户".to_string(),
    auto_reconnect: false,
    max_reconnect_attempts: 0,
}
```

### 测试超时设置

| 操作 | 超时时间 |
|------|---------|
| 连接服务器 | 15 秒 |
| 认证 | 10 秒 |
| 等待事件 | 5 秒 |
| 加入房间 | 10 秒 |
| 测试间延迟 | 2 秒 |

---

## 故障排除

### 测试失败常见原因

#### 1. HTTP 500 错误
```
Error: HTTP error: 500 Internal Server Error
```
**原因**: Cloudflare Workers Durable Objects 并发限制
**解决**: 等待几秒后重新运行测试

#### 2. 连接断开
```
Error: "客户端未连接，当前状态: Disconnected"
```
**原因**: WebSocket 连接异常断开
**解决**: 检查网络连接，重新运行测试

#### 3. 认证超时
```
Error: "客户端A认证超时"
```
**原因**: 服务器响应慢或网络延迟
**解决**: 增加超时时间或检查服务器状态

#### 4. 未收到预期事件
```
Error: "客户端B未收到A的鼠标位置更新"
```
**原因**: 
- 消息未发送成功
- 事件过滤条件不匹配
- 事件到达时间晚于超时

**解决**: 
- 检查发送代码
- 验证事件字段匹配
- 增加超时时间

---

## 扩展测试

### 添加新测试用例

在 `tests/collaboration_ui_test.rs` 中添加：

```rust
async fn test_your_new_feature_internal() -> Result<(), Box<dyn std::error::Error>> {
    println!("======================================================");
    println!("  测试新功能");
    println!("======================================================\n");
    
    // 1. 创建客户端A
    let collector_a = EventCollector::new();
    let mut client_a = CollaborationClient::new(ClientConfig {
        server_host: "lumino-02.afeu20u3jfocas.dpdns.org".to_string(),
        server_port: 80,
        username: "测试A".to_string(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_a.set_event_callback(collector_a.callback());
    
    // 2. 创建房间
    let create_result = client_a
        .create_room_and_connect("测试房间".to_string())
        .await?;
    
    // 3. 获取用户ID
    let user_a_id = collector_a
        .wait_for(|e| {
            if let CollaborationEvent::Authenticated { user_id, .. } = e {
                Some(user_id.clone())
            } else {
                None
            }
        }, 5000)
        .await
        .ok_or("认证超时")?;
    
    // 4. 客户端B加入...
    
    // 5. 测试新功能...
    
    Ok(())
}
```

然后在 `test_collaboration_ui_all()` 中添加调用：

```rust
match test_your_new_feature_internal().await {
    Ok(_) => { /* 记录成功 */ }
    Err(e) => { /* 记录失败 */ }
}
sleep(Duration::from_millis(2000)).await;
```

---

## CI 测试报告（JUnit / Pages）

CI 统一把 nextest 的结果落成 JUnit XML，并发布为网页报告：

- `.github/workflows/ci.yml` 每个平台（ubuntu/windows/macos）上传 `junit-<os>.xml` artifact；
- `.github/workflows/test-report.yml` 汇总这些 artifact，发布到 GitHub Pages：
  `https://<org>.github.io/<repo>/`（需在仓库 Settings → Pages 把 Source 设为 "GitHub Actions"）；
- nextest 0.9 已移除 `--junit-xml`，改为输出 libtest JSON 流后再转换。

本地复现同一套报告：

```bash
NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 cargo nextest run --workspace --no-fail-fast \
  --message-format libtest-json-plus > nextest.ndjson
python scripts/nextest_junit.py nextest.ndjson -o junit.xml --name "nextest (local)"
python scripts/junit_report.py <junit.xml 所在目录> -o site
```

---

## 相关文档

- [协作功能测试详细文档](./COLLABORATION_TEST.md) - 完整的协作测试说明
- [服务端日志](https://lumino-02.afeu20u3jfocas.dpdns.org/logs) - 实时服务器日志
- [项目 README](../README.md) - 项目总体说明

---

## 命令速查表

```bash
# 编译测试（不运行）
cargo nextest run --test collaboration_ui_test --no-run

# 运行 UI 测试（详细输出；默认 #[ignore]，需 --run-ignored only）
cargo nextest run --test collaboration_ui_test test_collaboration_ui_all --run-ignored only --no-capture

# 运行基础测试（默认 #[ignore] 的主测试）
cargo nextest run --test collaboration_full_test --run-ignored only --no-capture

# 运行特定测试函数（本地序列化单测，无需 --run-ignored）
cargo nextest run --test collaboration_full_test test_serialize_mouse_move --no-capture

# 不因某个测试失败而中止其余测试
cargo nextest run --test collaboration_ui_test --no-fail-fast

# 单线程运行（输出顺序稳定；--no-capture 本身即串行，-j 1 显式兜底）
cargo nextest run --test collaboration_ui_test --no-capture -j 1
```
