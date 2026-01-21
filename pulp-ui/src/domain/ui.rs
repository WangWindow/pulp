//! UI 相关枚举：页面、主题、视图模式等。

use rust_i18n::t;

/// Drawer 内部面板（同一时刻只显示一个，避免信息堆砌）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerPanel {
    Task,
    Extract,
    Rename,
    NewFolder,
    DeleteConfirm,
}

/// 顶层页面：主界面/设置界面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Browser,
    Settings,
}

/// 主题模式：默认跟随系统；也可手动锁定 Light/Dark。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

pub const THEME_MODES: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeMode::System => write!(f, "{}", t!("menu.settings.theme.follow_system")),
            ThemeMode::Light => write!(f, "{}", t!("menu.settings.theme.light")),
            ThemeMode::Dark => write!(f, "{}", t!("menu.settings.theme.dark")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// 文件系统：平铺列表（默认）。
    FileSystem,

    /// 文件系统：可展开的树状列表（用于“再展开”的目录视图）。
    FileSystemTree,

    /// 压缩包预览（虚拟文件视图/路径）：平铺。
    ///
    /// 说明：
    /// - “压缩包预览”本质也是文件视图：只是路径来自压缩包内部（虚拟路径），而不是磁盘；
    /// - 因此它应该与文件系统视图共享同一套“平铺/树状”的切换语义；
    /// - 渲染层只依赖“行信息”（path/is_dir/depth/显示名等），不关心真实来源。
    Archive,

    /// 压缩包预览（虚拟文件视图/路径）：树状列表（支持再展开）。
    ArchiveTree,
}
