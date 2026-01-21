//! 类型层常量：聚合 UI 状态常量，便于复用与统一调整。

/// 双击阈值：同一路径在该时间内连续点击视为双击。
pub const DOUBLE_CLICK_GAP_MS: u64 = 350;

/// Drawer（右侧抽屉）默认宽度与拖拽范围（像 IDE 一样可拖拽分割线）。
pub const DRAWER_DEFAULT_WIDTH_PX: f32 = 360.0;
pub const DRAWER_MIN_WIDTH_PX: f32 = 280.0;

/// 最大宽度采用窗口宽度的比例（避免喧宾夺主）。
pub const DRAWER_MAX_WIDTH_RATIO: f32 = 0.55;

/// 应用整体内边距。
pub const APP_PADDING_PX: f32 = 12.0;
/// 主要区域的间距。
pub const APP_GAP_PX: f32 = 10.0;
/// 侧边栏默认宽度。
pub const SIDEBAR_WIDTH_PX: f32 = 240.0;

/// 主列表（文件/树状/压缩包）固定行高：用于虚拟列表计算。
pub const LIST_ROW_HEIGHT_PX: f32 = 32.0;

/// 虚拟列表超前/滞后渲染行数（减少滚动抖动）。
pub const LIST_OVERSCAN: usize = 6;
