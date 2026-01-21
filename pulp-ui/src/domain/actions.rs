//! 交互动作与上下文操作类型。

/// 右键菜单/操作入口：使用强类型 Action，替代字符串，便于扩展与统一处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    Open,
    SmartExtract,
    ExtractTo,
    CompressZip,
    Rename,
    Delete,
    Properties,
}
