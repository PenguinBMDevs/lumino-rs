# 协作功能测试文档

本文档描述了 Lumino 协作功能的所有测试方法，包括基础功能测试和 UI 集成测试。

## 测试文件位置

| 测试文件 | 路径 | 描述 |
|---------|------|------|
| 基础协作测试 | `tests/collaboration_full_test.rs` | 基础连接和消息传递测试 |
| UI 集成测试 | `tests/collaboration_ui_test.rs` | 完整的双向同步测试套件 |

## 服务器信息

- **地址**: `lumino-02.afeu20u3jfocas.dpdns.org`
- **端口**: `80`
- **协议**: WebSocket (ws://)

## 测试类型

### 1. 基础协作测试 (collaboration_full_test.rs)

#### 测试场景
1. 用户01连接服务器并创建房间
2. 用户02使用邀请码加入房间
3. 用户01发送鼠标位置事件
4. 用户01发送音符创建事件
5. 用户02接收并验证这些事件

#### 运行方式

```bash
cargo test --test collaboration_full_test -- --nocapture
```

---

### 2. UI 集成测试 (collaboration_ui_test.rs)

这是主要的协作功能测试套件，包含三个子测试，顺序执行以避免服务器并发限制。

#### 测试架构

```
test_collaboration_ui_all (主入口)
├── test_mouse_cursor_sync_internal (测试 1/3)
│   ├── A 创建房间
│   ├── B 加入房间
│   ├── A→B 鼠标指针同步
│   ├── A→B 音符同步
│   ├── B→A 鼠标指针同步
│   └── B→A 音符同步
├── test_note_batch_sync_internal (测试 2/3)
│   ├── A 创建房间
│   ├── B 加入房间
│   └── A→B 批量音符同步 (3个音符)
└── test_mouse_movement_sync_internal (测试 3/3)
    ├── A 创建房间
    ├── B 加入房间
    └── A→B 连续鼠标移动同步 (5个位置点)
```

#### 详细测试内容

##### 测试 1: 鼠标指针同步 (test_mouse_cursor_sync_internal)

验证双向鼠标指针投影和音符同步功能。

**测试步骤：**
1. **客户端A连接并创建房间**
   - 创建协作客户端
   - 连接到服务器
   - 创建新房间
   - 获取邀请码

2. **客户端B加入房间**
   - 使用邀请码加入房间
   - 验证成功加入

3. **A→B 鼠标指针同步**
   - A 发送鼠标位置 (x=150, y=250)
   - B 接收并验证位置数据
   - 验证：用户ID、坐标值匹配

4. **A→B 音符同步**
   - A 创建音符 (tick=2880, key=65)
   - A 发送音符添加事件
   - B 接收并验证音符数据
   - 验证：tick、key、velocity 匹配

5. **B→A 鼠标指针同步**
   - B 发送鼠标位置 (x=300, y=400)
   - A 接收并验证位置数据

6. **B→A 音符同步**
   - B 创建音符 (tick=3840, key=72)
   - B 发送音符添加事件
   - A 接收并验证音符数据

**预期输出：**
```
✓ 客户端A连接并创建房间 - 通过
✓ 客户端B加入房间 - 通过
✓ A->B鼠标指针同步 - 通过 (投影可见)
✓ A->B音符同步 - 通过
✓ B->A鼠标指针同步 - 通过 (投影可见)
✓ B->A音符同步 - 通过
```

---

##### 测试 2: 音符批量同步 (test_note_batch_sync_internal)

验证批量音符操作同步功能。

**测试步骤：**
1. **客户端A创建房间**
   - 房间名称："批量测试房间"

2. **客户端B加入房间**

3. **批量音符同步**
   - A 创建 3 个音符：
     - 音符1: tick=0, key=60
     - 音符2: tick=480, key=64
     - 音符3: tick=960, key=67
   - A 发送批量添加操作
   - B 接收并验证收到全部 3 个音符

**预期输出：**
```
✓ 客户端A连接并创建房间 - 通过
✓ 客户端B加入房间 - 通过
✓ 客户端A发送批量音符 - 通过
✓ 客户端B接收批量音符 - 通过
```

---

##### 测试 3: 鼠标指针连续移动同步 (test_mouse_movement_sync_internal)

验证连续鼠标位置更新的实时同步。

**测试步骤：**
1. **客户端A创建房间**
   - 房间名称："移动测试房间"

2. **客户端B加入房间**

3. **连续移动同步**
   - A 依次发送 5 个鼠标位置：
     - 位置1: (100, 100)
     - 位置2: (200, 150)
     - 位置3: (300, 200)
     - 位置4: (400, 250)
     - 位置5: (500, 300)
   - 每个位置间隔 100ms
   - B 接收并验证最后一个位置

**预期输出：**
```
✓ 客户端A连接并创建房间 - 通过
✓ 客户端B加入房间 - 通过
✓ 鼠标指针连续移动同步 - 通过
```

#### 运行方式

```bash
cargo test --test collaboration_ui_test test_collaboration_ui_all -- --nocapture
```

---

## 测试结果解读

### 成功输出示例

```
======================================================
  协作UI集成测试套件
  服务器: lumino-02.afeu20u3jfocas.dpdns.org:80
======================================================

>>> 开始测试 1/3: 鼠标指针同步
...
✓ 测试 1/3 通过: 鼠标指针同步

>>> 开始测试 2/3: 音符批量同步
...
✓ 测试 2/3 通过: 音符批量同步

>>> 开始测试 3/3: 鼠标指针连续移动同步
...
✓ 测试 3/3 通过: 鼠标指针连续移动同步

======================================================
  测试总结
======================================================
✓ 鼠标指针同步
✓ 音符批量同步
✓ 鼠标指针连续移动同步
======================================================

🎉 所有测试通过！协作功能正常工作。
test test_collaboration_ui_all ... ok
test result: ok. 1 passed; 0 failed
```

### 失败情况处理

如果测试失败，通常会显示错误信息：

```
✗ 测试 X/3 失败: [测试名称] - [错误信息]
```

常见错误：
- `HTTP error: 500 Internal Server Error` - 服务器内部错误，可能是 Cloudflare Workers 限制
- `客户端未连接，当前状态: Disconnected` - 连接断开
- `认证超时` - 服务器响应慢或网络问题
- `未收到鼠标位置更新` - 消息传递失败

---

## 技术实现细节

### 事件收集器 (EventCollector)

所有测试使用 `EventCollector` 结构来捕获协作事件：

```rust
#[derive(Clone)]
struct EventCollector {
    events: Arc<Mutex<Vec<CollaborationEvent>>>,
}
```

**主要方法：**
- `callback()` - 生成事件回调函数
- `wait_for()` - 等待特定条件的事件
- `contains_event()` - 检查是否包含满足条件的事件

### 事件等待机制

测试使用轮询方式等待事件：

```rust
async fn wait_for<T, F>(&self, predicate: F, timeout_ms: u64) -> Option<T>
where
    F: Fn(&CollaborationEvent) -> Option<T>,
{
    let start = std::time::Instant::now();
    while start.elapsed().as_millis() < timeout_ms as u128 {
        // 检查事件列表
        // 如果匹配则返回
        sleep(Duration::from_millis(100)).await;
    }
    None
}
```

### 顺序执行机制

为避免 Cloudflare Workers 的并发限制，测试使用单入口顺序执行：

```rust
#[tokio::test]
async fn test_collaboration_ui_all() {
    // 测试1
    test_mouse_cursor_sync_internal().await?;
    sleep(Duration::from_millis(2000)).await; // 延迟
    
    // 测试2
    test_note_batch_sync_internal().await?;
    sleep(Duration::from_millis(2000)).await; // 延迟
    
    // 测试3
    test_mouse_movement_sync_internal().await?;
}
```

---

## 调试指南

### 服务端日志

访问服务器日志页面查看实时日志：
```
https://lumino-02.afeu20u3jfocas.dpdns.org/logs
```

### 测试输出中的关键信息

测试输出包含以下诊断信息：

1. **HTTP 响应**：
   ```
   [HTTP] Response: {"success":true,"room":{...}}
   ```

2. **WebSocket 认证响应**：
   ```
   [WS] Received auth response: {"type":"authenticated",...}
   ```

3. **事件接收**：
   ```
   [Event] 收到事件: MouseUpdate { user_id: "...", position: ... }
   ```

### 常见问题排查

#### 问题1: 500 Internal Server Error

**原因**: Cloudflare Workers Durable Objects 限制
**解决**: 等待几秒后重试测试

#### 问题2: 认证超时

**原因**: 网络延迟或服务器负载高
**解决**: 增加超时时间或检查网络连接

#### 问题3: 未收到事件

**原因**: 
- 事件过滤器不匹配
- 事件在超时后才到达
- WebSocket 连接断开

**解决**: 
- 检查事件类型和字段
- 增加超时时间
- 检查连接状态

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `tests/collaboration_full_test.rs` | 基础功能测试 |
| `tests/collaboration_ui_test.rs` | UI 集成测试套件 |

| `crates/collaboration/src/client.rs` | 协作客户端实现 |
| `crates/ui/src/editor/grid/remote_cursors.rs` | 远程光标渲染 |



### 使用说明

1. 启动程序后，点击"连接服务器"按钮
2. 程序会自动创建一个房间并让两个客户端加入
3. 在左侧面板移动鼠标，观察右侧面板的远程光标
4. 点击面板创建音符，观察对方面板的音符同步
5. 底部事件日志显示所有协作事件

### 界面布局

```
+---------------------------+---------------------------+
|      Client A (Alice)     |      Client B (Bob)       |
|      Color: #FF6B6B       |      Color: #4ECDC4       |
|                           |                           |
|  [Canvas Area]            |  [Canvas Area]            |
|  - Move mouse to sync     |  - Move mouse to sync     |
|  - Click to add note      |  - Click to add note      |
|                           |                           |
|  Notes: 0 | Cursors: 0    |  Notes: 0 | Cursors: 0    |
+---------------------------+---------------------------+
| [连接服务器] | 状态: 已连接 | 房间: xxx | 在线: 2       |
+-------------------------------------------------------+
| 操作: 在面板上移动鼠标同步光标 | 点击面板创建音符      |
+-------------------------------------------------------+
| [12:34:56] 开始连接服务器...                          |
| [12:34:57] 房间已创建: xxx                            |
| [12:34:58] 远程光标: Bob (150, 250)                   |
+-------------------------------------------------------+
```

---

## 更新历史

- **2026-03-27**: 创建 UI 集成测试套件，包含双向同步测试
- **2026-03-27**: 更新服务器地址为 lumino-02.afeu20u3jfocas.dpdns.org:80
- **2026-03-27**: 实现顺序执行机制避免并发限制
