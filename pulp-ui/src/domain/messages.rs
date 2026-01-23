//! iced 消息定义：UI 与状态的唯一通信通道。

use super::{ContextAction, EntryRow, FileEntry, ThemeMode};
use crate::i18n;
use iced::event;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Message {
    #[allow(dead_code)]
    Noop,

    // -------------------------------
    // 导航与菜单
    // -------------------------------
    ToggleMenu,
    ToggleTitleMenu,
    DismissTitleMenu,

    /// 专用：关闭文件条目右键菜单
    DismissContextMenu,
    NavigateBack,
    NavigateForward,
    NavigateUp,
    NavigateHome,
    NavigateTo(PathBuf),

    // -------------------------------
    // Settings / Preferences
    //
    // 设计说明（对实现有用的约束）：
    // - 语言偏好存储在配置文件中（TOML），UI 允许“跟随系统 / 固定语言”两种模式。
    // - “回滚”语义：把偏好切回 FollowSystem，并立即重新按系统语言计算 effective locale。
    // - Message 层不直接做 IO：load/save 由 app/update 层触发（可异步），并在成功后回传。
    // -------------------------------
    ToggleSettings,
    ToggleLocationEdit,
    ThemeModeChanged(ThemeMode),

    /// 设置：用户选择了语言偏好（跟随系统 or 固定 en/zh-CN）。
    LocalePreferenceChanged(i18n::LocalePreference),
    /// 设置：用户请求“回滚到系统语言”（等价于把偏好设为 FollowSystem）。
    LocaleRollbackToSystem,

    PathChanged(String),
    PathSubmitted,
    FilterChanged(String),

    /// 行点击：统一既支持文件系统，也支持压缩包虚拟视图。
    RowClicked(EntryRow),

    /// 文件系统目录加载完成：下一步会在 state 里把它转换为 `Vec<EntryRow>` 供渲染层使用。
    DirLoaded(PathBuf, Vec<FileEntry>),

    /// 压缩包条目加载完成：下一步会转换为 `Vec<EntryRow>`，并生成 `archive.zip::/path` 虚拟路径。
    ArchiveLoaded(Result<(PathBuf, Vec<pulp_core::ArchiveEntry>), String>),

    ExtractFinished(Result<PathBuf, String>),
    ExtractToRequested(PathBuf),
    ExtractToChanged(String),
    ExtractToConfirm,
    ExtractToCancel,
    CompressSelected,
    CompressFinished(Result<PathBuf, String>),

    NewFolderRequested,
    NewFolderChanged(String),
    NewFolderConfirm,
    NewFolderCancel,
    FolderCreated(Result<PathBuf, String>),

    RenameRequested(PathBuf),
    RenameChanged(String),
    RenameConfirm,
    RenameCancel,
    Renamed(Result<PathBuf, String>),

    DeleteRequested(PathBuf),
    DeleteConfirm,
    DeleteCancel,
    DeleteFinished(Result<(), String>),

    /// 打开文件/路径完成
    OpenFinished(Result<(), String>),

    /// 属性对话框
    PropertiesRequested(EntryRow),
    PropertiesClose,

    /// 强类型右键/上下文操作（替代字符串 action）。
    ContextActionFor(ContextAction, EntryRow),

    /// 磁盘挂载/卸载（Linux: udisksctl）
    MountRequested(String),
    UnmountRequested(String),
    UnmountConfirmRequested(String),
    UnmountConfirmCancel,
    UnmountConfirmAccept,
    MountFinished(Result<PathBuf, String>),
    UnmountFinished(Result<(), String>),

    // Drawer（右侧抽屉）
    CloseDrawer,
    DrawerResizeStart,

    // -------------------------------
    // 树状展开列表（FileSystemTree）相关
    //
    // 目标：
    // - “同一个主列表区域”既能平铺，也能树状展开；
    // - 展开状态在 UI 层维护，子目录按需加载；
    // - 点击目录行的“展开箭头”只展开/折叠，不触发打开；
    // - 双击目录行仍然是“导航进入该目录”（与平铺一致）。
    // -------------------------------
    /// 切换文件视图模式：平铺 / 树状列表。
    ToggleFileViewMode,

    /// 展开/折叠某个目录节点（树状列表用）。
    ///
    /// `expanded = true` 表示想展开；`false` 表示折叠。
    TreeToggle(PathBuf, bool),

    /// 某个目录的子项已加载完成（树状列表用）。
    ///
    /// 说明：
    /// - `children` 是该目录的直接子项（不递归）；
    //  - 由 app 发起异步加载，完成后回传到此消息，更新 UI 的虚拟树结构。
    TreeChildrenLoaded(PathBuf, Vec<FileEntry>),

    // 单任务模型：进度与可取消（用于 Drawer::Task 面板）
    TaskCancelRequested,

    Tick,
    /// 文件视图自动刷新
    AutoRefreshTick,
    Event(event::Event),
    /// 主列表滚动视口变更（用于虚拟列表）。
    ListViewportChanged(iced::widget::scrollable::Viewport),

    /// 侧边栏左划（触屏/鼠标拖拽）
    SidebarSwipeStart(String, PathBuf),
}
