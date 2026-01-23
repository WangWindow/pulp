//! 跨平台（Windows / Linux）位置与磁盘卷（挂载点）服务
//!
//! 设计原则：
//! - UI 不关心平台差异：统一输出 SidebarItem 列表
//! - 数据尽量“可用优先”：拿不到标签/容量也能正常显示与导航
//! - 为后续扩展保留字段：uuid、fs_type、removable、network 等
//!
//! 依赖：sysinfo（跨平台获取磁盘与挂载点信息）

use crate::utils::format_size;
use rust_i18n::t;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
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

    /// 设备标识（Linux: /dev/sda1），用于挂载/卸载等操作。
    pub device: Option<String>,

    /// 文件系统类型（如 ext4 / xfs / btrfs）。
    pub fs_type: Option<String>,

    /// 是否可移除设备（用于显示与操作提示）。
    pub removable: Option<bool>,

    /// 是否已挂载（某些条目可能未挂载但可挂载）。
    pub mounted: bool,

    /// 是否系统盘（例如 Linux 的 "/"）。
    pub system: bool,

    /// 是否可写（目前仅用于 UI 可选的禁用态；不保证完全准确）。
    pub writable: Option<bool>,
}

static SIDEBAR_ITEMS_CACHE: OnceLock<Mutex<HashMap<String, Vec<SidebarItem>>>> = OnceLock::new();
static MOUNT_SUPPORT_CACHE: OnceLock<bool> = OnceLock::new();

/// 是否支持磁盘挂载（Linux: udisksctl）。
///
/// - 若系统缺少 `udisksctl`，返回 false。
/// - 非 Linux 平台直接返回 false。
pub fn mount_supported() -> bool {
    *MOUNT_SUPPORT_CACHE.get_or_init(|| {
        if cfg!(target_os = "linux") {
            command_exists("udisksctl")
        } else {
            false
        }
    })
}

/// 清空侧边栏缓存（挂载/卸载后用于刷新）。
pub fn invalidate_sidebar_cache() {
    if let Some(cache) = SIDEBAR_ITEMS_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            guard.clear();
        }
    }
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

/// 基于 locale 的缓存版本（避免重复生成本地化文本/系统调用）。
pub fn load_sidebar_items_cached(locale_key: &str) -> Vec<SidebarItem> {
    let cache = SIDEBAR_ITEMS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().expect("sidebar items cache lock");

    if let Some(items) = guard.get(locale_key) {
        return items.clone();
    }

    let items = load_sidebar_items();
    guard.insert(locale_key.to_string(), items.clone());
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
            device: None,
            fs_type: None,
            removable: None,
            mounted: true,
            system: false,
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

    // // 根目录：Linux 标配；Windows 上如果存在也无妨（通常不会是用户可访问的“/”）。
    // add_if_dir(
    //     &mut out,
    //     SidebarItemKind::Place,
    //     t!("sidebar.root").to_string(),
    //     PathBuf::from("/"),
    // );

    out
}

/// 采集磁盘卷/挂载点（跨平台），使用 sysinfo。
fn load_volumes() -> Vec<SidebarItem> {
    #[cfg(target_os = "linux")]
    {
        if mount_supported() && command_exists("lsblk") {
            if let Ok(items) = load_volumes_lsblk() {
                if !items.is_empty() {
                    return items;
                }
            }
        }
    }

    load_volumes_sysinfo()
}

fn load_volumes_sysinfo() -> Vec<SidebarItem> {
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

        let fs_type = disk
            .file_system()
            .to_string_lossy()
            .to_string()
            .trim()
            .to_string();

        let device = disk.name().to_string_lossy().to_string();
        let removable = Some(disk.is_removable());

        out.push(SidebarItem {
            kind: SidebarItemKind::Volume,
            label,
            subtitle,
            path: mount.clone(),
            device: if device.trim().is_empty() {
                None
            } else {
                Some(device)
            },
            fs_type: if fs_type.is_empty() {
                None
            } else {
                Some(fs_type)
            },
            removable,
            mounted: true,
            system: is_system_mount_path(&mount),
            writable: Some(is_writable(&mount)),
        });
    }

    out
}

#[cfg(target_os = "linux")]
fn load_volumes_lsblk() -> Result<Vec<SidebarItem>, String> {
    let output = Command::new("lsblk")
        .args(["-J", "-o", "NAME,PATH,MOUNTPOINT,LABEL,FSTYPE,RM,SIZE,TYPE"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let parsed: LsblkOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("lsblk json parse failed: {e}"))?;

    let mut out = Vec::new();
    for dev in parsed.blockdevices.into_iter() {
        flatten_lsblk(dev, &mut out);
    }

    Ok(out)
}

#[cfg(target_os = "linux")]
fn flatten_lsblk(dev: LsblkDevice, out: &mut Vec<SidebarItem>) {
    let dev_type = dev.dev_type.unwrap_or_default();
    let path = dev.path.unwrap_or_default();
    let mountpoint = dev.mountpoint.clone().unwrap_or_default();

    if dev_type == "loop" || path.trim().is_empty() {
        if let Some(children) = dev.children {
            for c in children {
                flatten_lsblk(c, out);
            }
        }
        return;
    }

    if !dev.fstype.as_deref().unwrap_or("").is_empty() {
        let mounted = !mountpoint.trim().is_empty();
        let mount_path = if mounted {
            PathBuf::from(mountpoint.trim())
        } else {
            PathBuf::from(path.clone())
        };

        if mounted && should_filter_mount_point(&mount_path) {
            return;
        }

        let label = dev
            .label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                if mounted {
                    mount_path.display().to_string()
                } else {
                    dev.name.clone().unwrap_or_else(|| path.clone())
                }
            });

        let mut subtitle_parts = Vec::new();
        if let Some(size) = dev.size.clone().filter(|s| !s.trim().is_empty()) {
            subtitle_parts.push(size);
        }
        if !mounted {
            subtitle_parts.push(t!("sidebar.unmounted").to_string());
        }

        let subtitle = if subtitle_parts.is_empty() {
            None
        } else {
            Some(subtitle_parts.join(" · "))
        };

        out.push(SidebarItem {
            kind: SidebarItemKind::Volume,
            label,
            subtitle,
            path: mount_path.clone(),
            device: Some(path.clone()),
            fs_type: dev.fstype.clone(),
            removable: dev.rm,
            mounted,
            system: mounted && is_system_mount_path(&mount_path),
            writable: if mounted {
                Some(is_writable(&mount_path))
            } else {
                None
            },
        });
    }

    if let Some(children) = dev.children {
        for c in children {
            flatten_lsblk(c, out);
        }
    }
}

#[derive(Debug, Deserialize)]
struct LsblkOutput {
    blockdevices: Vec<LsblkDevice>,
}

#[derive(Debug, Deserialize, Clone)]
struct LsblkDevice {
    name: Option<String>,
    path: Option<String>,
    mountpoint: Option<String>,
    label: Option<String>,
    fstype: Option<String>,
    rm: Option<bool>,
    size: Option<String>,
    #[serde(rename = "type")]
    dev_type: Option<String>,
    children: Option<Vec<LsblkDevice>>,
}

fn command_exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 挂载设备（Linux: udisksctl）。
pub async fn mount_device(device: String) -> Result<PathBuf, String> {
    if !mount_supported() {
        return Err("mount unsupported".to_string());
    }

    if !cfg!(target_os = "linux") {
        return Err("mount unsupported on this platform".to_string());
    }

    tokio::task::spawn_blocking(move || {
        let output = Command::new("udisksctl")
            .args(["mount", "-b", device.as_str()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(parse_udisksctl_mount_path(&stdout).unwrap_or_default())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 卸载设备（Linux: udisksctl）。
pub async fn unmount_device(device: String) -> Result<(), String> {
    if !mount_supported() {
        return Err("unmount unsupported".to_string());
    }

    if !cfg!(target_os = "linux") {
        return Err("unmount unsupported on this platform".to_string());
    }

    tokio::task::spawn_blocking(move || {
        let output = Command::new("udisksctl")
            .args(["unmount", "-b", device.as_str()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn parse_udisksctl_mount_path(stdout: &str) -> Option<PathBuf> {
    // 典型输出："Mounted /dev/sdb1 at /run/media/user/Label."
    let marker = " at ";
    let idx = stdout.find(marker)?;
    let mut path = stdout[idx + marker.len()..].trim();
    if let Some(stripped) = path.strip_suffix('.') {
        path = stripped.trim();
    }
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
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
fn format_capacity(avail: u64, total: u64, _disk: &sysinfo::Disk) -> String {
    let avail_s = format_size(avail);
    let total_s = format_size(total);

    t!(
        "sidebar.capacity.available",
        avail = avail_s,
        total = total_s
    )
    .to_string()
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
            device: None,
            fs_type: None,
            removable: None,
            mounted: true,
            system: false,
            writable: Some(is_writable(&path)),
        });
    }
}

/// 是否系统盘挂载点（Linux 常见）。
fn is_system_mount_path(path: &Path) -> bool {
    matches!(path.to_string_lossy().as_ref(), "/" | "/boot" | "/boot/efi")
}

/// 是否系统盘设备（用于阻止卸载）。
pub fn is_system_device(device: &str) -> bool {
    load_sidebar_items()
        .iter()
        .any(|item| item.device.as_deref() == Some(device) && item.system)
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
