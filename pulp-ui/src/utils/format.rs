//! 通用格式化工具（尺寸/时间/文件名）。

use std::path::Path;

/// 将字节数格式化为更友好的显示（KB/MB/GB）。
pub fn format_size(size: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", size, units[unit])
    } else {
        format!("{:.1} {}", value, units[unit])
    }
}

/// 将时间格式化为 `YYYY-MM-DD HH:mm`；失败时返回占位符。
pub fn format_time(time: Option<std::time::SystemTime>) -> String {
    let fmt = "[year]-[month]-[day] [hour]:[minute]";
    let fmt = match time::format_description::parse(fmt) {
        Ok(f) => f,
        Err(_) => return "—".into(),
    };

    time.and_then(|t| time::OffsetDateTime::from(t).format(&fmt).ok())
        .unwrap_or_else(|| "—".into())
}

/// 获取压缩包文件名的 stem（支持 `.tar.gz/.tar.bz2/.tar.xz`）。
pub fn archive_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "archive".into());
    if name.ends_with(".tar.gz") {
        return name.trim_end_matches(".tar.gz").to_string();
    }
    if name.ends_with(".tar.bz2") {
        return name.trim_end_matches(".tar.bz2").to_string();
    }
    if name.ends_with(".tar.xz") {
        return name.trim_end_matches(".tar.xz").to_string();
    }
    Path::new(&name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or(name)
}
