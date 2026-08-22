//! 协作相关事件

#[derive(Debug, Clone)]
/// 协作事件
pub enum Event {
    /// 连接协作服务器
    Connect {
        /// 服务器主机地址
        host: String,
        /// 服务器端口
        port: u16,
        /// 协作用户名
        username: String,
        /// 密码（与注册/登录账户一致，用于 WebSocket 握手鉴权）
        password: String,
        /// 邀请码（加入已有房间时提供）
        invite_code: Option<String>,
    },
    /// 创建协作房间
    CreateRoom {
        /// 房间名称
        name: String,
    },
    /// 加入协作房间
    JoinRoom {
        /// 房间邀请码
        invite_code: String,
    },
    /// 断开协作连接
    Disconnect,
    /// 协作认证成功
    Authenticated {
        /// 认证用户 ID
        user_id: String,
        /// 邀请码
        invite_code: String,
    },
    /// 协作房间创建成功
    RoomCreated {
        /// 房间名称
        room_name: String,
        /// 邀请码
        invite_code: String,
        /// 工程名称（host 上传时用）
        project_name: String,
        /// 工程哈希（host 上传时用，hex）
        project_hash: String,
    },
    /// 协作加入房间成功
    RoomJoined {
        /// 房间名称
        room_name: String,
        /// 邀请码
        invite_code: String,
        /// 房间用户数
        user_count: usize,
        /// 工程名称
        project_name: String,
        /// 工程哈希（hex），用于判断与本地工程是否一致
        project_hash: String,
    },
    /// 协作连接断开
    Disconnected,
    /// 协作连接失败（携带原因，用于驱动对话框回到可重试状态）
    ConnectFailed {
        /// 失败原因
        reason: String,
    },
    /// 协作用户离开
    UserLeft {
        /// 离开的用户 ID
        user_id: String,
    },
    /// 协作鼠标位置更新
    MouseUpdate {
        /// 远端用户 ID
        user_id: String,
        /// 横向坐标
        x: f32,
        /// 纵向坐标
        y: f32,
        /// 远端用户颜色
        color: String,
        /// 远端用户名
        username: String,
    },
    /// 协作音符更新（来自其他用户）
    NoteUpdate {
        /// 远端用户 ID
        user_id: String,
        /// 更新操作（NoteBatchOperation 的 JSON 字符串）
        operation: String,
    },
    /// 协作工程更新（来自其他用户，如音轨变更）
    ProjectUpdate {
        /// 远端用户 ID
        user_id: String,
        /// 工程更新内容（ProjectUpdate 的 JSON 字符串）
        update: String,
    },
    /// 远端选择更新（来自其他用户的本地选择变更）
    Selection {
        /// 远端用户 ID
        user_id: String,
        /// 选择内容（JSON 字符串：{active, timestamp, fingerprints}）
        selection: String,
        /// 远端用户颜色（hex 字符串，可能为空，由接收方按 user_id 派生）
        color: String,
    },
}
