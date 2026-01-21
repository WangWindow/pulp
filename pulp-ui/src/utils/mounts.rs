//! 跨平台（Windows / Linux）位置与磁盘卷（挂载点）服务
//!
//! 设计原则：
//! - UI 不关心平台差异：统一输出 SidebarItem 列表
//! - 数据尽量“可用优先”：拿不到标签/容量也能正常显示与导航
//! - 为后续扩展保留字段：uuid、fs_type、removable、network 等
//!
//! 依赖：sysinfo（跨平台获取磁盘与挂载点信息）

use rust_i18n::t;
use std::path::{Path, PathBuf};
use sysinfo::Disks;

/// 左侧栏条目类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SidebarItemKind {
    /// 常用位置
    Place,
    /// 磁盘/卷（Windows 驱动器 / Linux 挂载）
    Volume,
}

/// 左侧栏条目（UI 使用它渲染按钮/列表项）。
///
/// 说明：
/// - 该结构体是“服务层 → 视图层”的稳定契约（接口），后续扩展字段时尽量保持向后兼容。
#[derive(Debug, Clone)]
pub struct SidebarItem {
    /// 条目类型：Place / Volume
    pub kind: SidebarItemKind,

    /// UI 显示名称，例如：
    /// - Place：主目录/下载/桌面
    /// - Volume：C: (系统) / / /mnt/data
    pub label: String,

    /// 次要信息，例如：
    /// - Volume：总容量/可用容量
    /// - Linux：设备名、文件系统类型
    pub subtitle: Option<String>,

    /// 点击后要导航到的路径。
    pub path: PathBuf,

    /// 是否可写（目前仅用于 UI 可选的禁用态；不保证完全准确）。
    pub writable: Option<bool>,
}

/// 采集侧边栏数据：Places + Volumes。
///
/// 设计约定（符合 Nautilus：不显示“此电脑”）：
/// - 视图层应直接渲染 `Places` 与 `Volumes` 两组
/// - Windows：Volumes 直接列出各驱动器（例如 C:, D:），不额外提供“此电脑”入口
/// - Linux：Volumes 直接列出各挂载点
pub fn load_sidebar_items() -> Vec<SidebarItem> {
    let mut items = Vec::new();

    items.extend(load_places());
    items.extend(load_volumes());

    // 简单排序：Places 在前，Volumes 在后；组内按 label 排序
    items.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });

    items
}

/// 生成常用位置（跨平台）。
fn load_places() -> Vec<SidebarItem> {
    let mut out = Vec::new();

    // HOME：Linux/Windows 均可能存在（Windows 常见为 C:\Users\xxx）
    let home = std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        // Windows 上 HOME 未必存在，尝试 USERPROFILE
        .or_else(|| {
            std::env::var("USERPROFILE")
                .ok()
                .map(PathBuf::from)
                .filter(|p| p.is_dir())
        });

    if let Some(home) = home {
        out.push(SidebarItem {
            kind: SidebarItemKind::Place,
            label: t!("sidebar.home").to_string(),
            subtitle: Some(home.display().to_string()),
            path: home.clone(),
            writable: Some(is_writable(&home)),
        });

        // 以下目录名尽量参考常见默认值；不存在则跳过。
        add_if_dir(
            &mut out,
            SidebarItemKind::Place,
            t!("sidebar.desktop").to_string(),
            home.join("Desktop"),
        );
        add_if_dir(
            &mut out,
            SidebarItemKind::Place,
            t!("sidebar.downloads").to_string(),
            home.join("Downloads"),
        );
        add_if_dir(
            &mut out,
            SidebarItemKind::Place,
            t!("sidebar.documents").to_string(),
            home.join("Documents"),
        );

        // Linux 下常见“图片/音乐/视频”，Windows 也可能存在（视用户配置）。
        add_if_dir(
            &mut out,
            SidebarItemKind::Place,
            t!("sidebar.pictures").to_string(),
            home.join("Pictures"),
        );
        add_if_dir(
            &mut out,
            SidebarItemKind::Place,
            t!("sidebar.music").to_string(),
            home.join("Music"),
        );
        add_if_dir(
            &mut out,
            SidebarItemKind::Place,
            t!("sidebar.videos").to_string(),
            home.join("Videos"),
        );
    }

    // 根目录：Linux 标配；Windows 上如果存在也无妨（通常不会是用户可访问的“/”）。
    add_if_dir(
        &mut out,
        SidebarItemKind::Place,
        t!("sidebar.root").to_string(),
        PathBuf::from("/"),
    );

    out
}

/// 采集磁盘卷/挂载点（跨平台），使用 sysinfo。
fn load_volumes() -> Vec<SidebarItem> {
    // sysinfo：使用 Disks API（跨平台），并刷新列表与容量数据
    let mut disks = Disks::new_with_refreshed_list();
    for disk in disks.list_mut() {
        // 刷新容量等信息
        disk.refresh();
    }

    let mut out = Vec::new();

    for disk in disks.list() {
        // mount_point：
        // - Windows：通常是 "C:\"
        // - Linux：通常是 "/"、"/home"、"/mnt/xxx"
        let mount = disk.mount_point().to_path_buf();

        // 过滤掉明显的“非用户浏览目标”的挂载点（尽量保守）。
        if should_filter_mount_point(&mount) {
            continue;
        }

        let total = disk.total_space();
        let avail = disk.available_space();

        let label = volume_label(disk, &mount);
        let subtitle = Some(format_capacity(avail, total, disk));

        out.push(SidebarItem {
            kind: SidebarItemKind::Volume,
            label,
            subtitle,
            path: mount.clone(),
            writable: Some(is_writable(&mount)),
        });
    }

    out
}

/// 根据平台习惯生成卷显示名。
///
/// Windows：
/// - 优先 "C:" 或 "C:" + " (Label)"
///
/// Linux：
/// - 优先 mount path（例如 "/"、"/home"）
/// - 若能拿到设备名/文件系统信息，则放 subtitle
fn volume_label(disk: &sysinfo::Disk, mount: &Path) -> String {
    let mount_str = mount.display().to_string();

    // sysinfo 的 name()：
    // - Windows：通常是卷标或设备名
    // - Linux：可能是设备名（如 /dev/sda1）
    let name = disk.name().to_string_lossy().to_string();

    // Windows 盘符：mount_point 通常以 "X:\" 形式出现。
    if let Some(drive) = windows_drive_letter(mount_str.as_str()) {
        let label = name.trim();
        if label.is_empty() || label.eq_ignore_ascii_case(&drive) {
            return drive;
        }
        return format!("{drive} ({label})");
    }

    // Linux/其它：直接用挂载点路径作为 label
    // 若 name 与 mount 一样或空则忽略。
    if name.trim().is_empty() || name.trim() == mount_str {
        mount_str
    } else {
        // 也可以用 "{mount} ({name})"；但 Nautilus 更倾向显示挂载点/卷名，
        // 这里选择保守：label 仍然是 mount，name 放到 subtitle。
        mount_str
    }
}

/// Windows：从 "C:\" 提取 "C:"。
fn windows_drive_letter(mount_str: &str) -> Option<String> {
    let bytes = mount_str.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        let c = mount_str.chars().next()?;
        if c.is_ascii_alphabetic() {
            return Some(format!("{}:", c.to_ascii_uppercase()));
        }
    }
    None
}

/// 格式化容量信息（可用/总计），并尽量补充文件系统信息（若可得）。
fn format_capacity(avail: u64, total: u64, disk: &sysinfo::Disk) -> String {
    let avail_s = format_bytes(avail);
    let total_s = format_bytes(total);

    // sysinfo Disk 的 file_system() 返回 OsStr（例如 "NTFS"/"ext4"）
    let fs = disk.file_system().to_string_lossy().to_string();
    if fs.trim().is_empty() {
        t!(
            "sidebar.capacity.available",
            avail = avail_s,
            total = total_s
        )
        .to_string()
    } else {
        t!(
            "sidebar.capacity.available_fs",
            avail = avail_s,
            total = total_s,
            fs = fs
        )
        .to_string()
    }
}

/// 是否应过滤该挂载点（主要用于 Linux）。
///
/// 原则：不过度过滤，避免误伤真实盘。
fn should_filter_mount_point(mount: &Path) -> bool {
    let s = mount.to_string_lossy();

    // Linux 常见虚拟挂载目录
    // 例如：/proc, /sys, /dev, /run
    for prefix in ["/proc", "/sys", "/dev", "/run"] {
        if s == prefix || s.starts_with(&format!("{prefix}/")) {
            if prefix == "/run" && (s == "/run/media" || s.starts_with("/run/media/")) {
                return false;
            }
            return true;
        }
    }

    // 其它：不过滤
    false
}

/// 若路径存在且是目录，加入侧栏。
fn add_if_dir(out: &mut Vec<SidebarItem>, kind: SidebarItemKind, label: String, path: PathBuf) {
    if path.is_dir() {
        out.push(SidebarItem {
            kind,
            label,
            subtitle: Some(path.display().to_string()),
            path: path.clone(),
            writable: Some(is_writable(&path)),
        });
    }
}

/// 粗略判断一个路径是否可写。
///
/// 注意：这不是权限系统的完整判断，只用于 UI 的“禁用/提示”。
fn is_writable(path: &Path) -> bool {
    // 试图在目录中创建临时文件名可能代价较高；这里采用 metadata 的 readonly 粗略判断。
    // Windows/Linux 上都不完全准确，但足够用于“弱提示”。
    std::fs::metadata(path)
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false)
}

/// 人类可读格式的字节数。
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_letter_parse() {
        assert_eq!(windows_drive_letter("C:\\").as_deref(), Some("C:"));
        assert_eq!(windows_drive_letter("d:\\").as_deref(), Some("D:"));
        assert_eq!(windows_drive_letter("/").as_deref(), None);
    }
}
