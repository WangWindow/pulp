//! UI 配置（跨平台持久化）

use crate::i18n::{LocalePreference, LocaleSettingValue};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// 配置文件名（放在 ProjectDirs::config_dir 下）。
const CONFIG_FILE_NAME: &str = "config.toml";

/// 当前配置 schema 版本。
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Pulp UI 配置（对外类型）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    /// 配置版本号，用于未来迁移。
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    /// 语言设置：system / 固定 locale（en / zh-CN）。
    #[serde(default)]
    pub locale: LocaleSettingValue,
    // 预留：未来可加入 theme、drawer_width、view_mode 等。
}

fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            // 默认跟随系统语言（回滚的目标状态也应是这个值）
            locale: LocaleSettingValue::default(),
        }
    }
}

impl AppConfig {
    /// 将配置中的 locale 设置映射为业务偏好（FollowSystem/Fixed）。
    pub fn locale_preference(&self) -> LocalePreference {
        self.locale.to_preference()
    }

    /// 更新语言偏好（会影响后续保存）。
    pub fn set_locale_preference(&mut self, pref: LocalePreference) {
        self.locale = LocaleSettingValue::from_preference(pref);
    }
}

/// 查找/创建配置目录与配置文件路径。
///
/// 路径规则（由 directories 决定）：
/// - Linux: ~/.config/pulp/pulp/config.toml（具体取决于平台约定）
/// - Windows: %APPDATA%\\pulp\\pulp\\config.toml
/// - macOS: ~/Library/Application Support/pulp/pulp/config.toml
pub fn config_path() -> io::Result<PathBuf> {
    let dirs = project_dirs().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "No valid home/config directory found for this platform",
        )
    })?;

    Ok(dirs.config_dir().join(CONFIG_FILE_NAME))
}

/// 获取 ProjectDirs（跨平台标准目录）。
///
/// 注意：
/// - qualifier/organization/application 用于生成目录层级。
/// - 这里选择相对稳定且不易冲突的值。
fn project_dirs() -> Option<ProjectDirs> {
    // qualifier: 反向域名/组织标识；organization + application
    ProjectDirs::from("dev", "pulp", "pulp")
}

/// 从磁盘加载配置（若不存在则返回默认值）。
///
/// 行为说明：
/// - 文件不存在：返回 Default，不写盘（由上层决定是否立即写回）。
/// - 解析失败：返回错误（上层可提示用户并选择“重置为默认”）。
/// - schema_version 旧：自动迁移到最新结构（仅在内存中），由上层决定是否保存回磁盘。
pub fn load() -> io::Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let raw = fs::read_to_string(&path)?;
    let mut cfg: AppConfig = toml::from_str(&raw).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse config TOML: {e}"),
        )
    })?;

    migrate_in_place(&mut cfg);

    Ok(cfg)
}

/// 将配置写入磁盘（保证父目录存在）。
///
/// 行为说明：
/// - 会创建父目录（若不存在）。
/// - 以“覆盖写”更新 config.toml。
pub fn save(cfg: &AppConfig) -> io::Result<()> {
    let path = config_path()?;
    ensure_parent_dir(&path)?;

    // 保存前确保 schema_version 是最新（避免写出旧版本）。
    let mut cfg = cfg.clone();
    cfg.schema_version = CURRENT_SCHEMA_VERSION;

    let toml = toml::to_string_pretty(&cfg).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize config to TOML: {e}"),
        )
    })?;

    fs::write(path, toml)
}

/// 迁移：把旧配置就地升级到当前结构。
///
/// 说明：
/// - 目前 schema_version=1，没有历史版本；但保留迁移入口，避免未来改动牵一发动全身。
fn migrate_in_place(cfg: &mut AppConfig) {
    // 未来迁移示例：
    // if cfg.schema_version == 0 {
    //     ...转换旧字段...
    //     cfg.schema_version = 1;
    // }
    if cfg.schema_version > CURRENT_SCHEMA_VERSION {
        // 配置来自未来版本：保守处理
        // - 不尝试降级迁移
        // - 允许继续运行（上层可选择提示用户）
        return;
    }

    // 当前版本：确保缺省值补全
    if cfg.schema_version == 0 {
        cfg.schema_version = 1;
    }
}

/// 确保路径的父目录存在。
fn ensure_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{AppLocale, LocalePreference};

    #[test]
    fn default_config_is_system_locale() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.locale_preference(), LocalePreference::FollowSystem);
    }

    #[test]
    fn set_locale_preference_roundtrip() {
        let mut cfg = AppConfig::default();
        cfg.set_locale_preference(LocalePreference::Fixed(AppLocale::ZhCn));
        assert_eq!(
            cfg.locale_preference(),
            LocalePreference::Fixed(AppLocale::ZhCn)
        );

        cfg.set_locale_preference(LocalePreference::FollowSystem);
        assert_eq!(cfg.locale_preference(), LocalePreference::FollowSystem);
    }

    #[test]
    fn migrate_keeps_future_schema() {
        let mut cfg = AppConfig {
            schema_version: CURRENT_SCHEMA_VERSION + 10,
            locale: LocaleSettingValue::default(),
        };
        migrate_in_place(&mut cfg);
        assert_eq!(cfg.schema_version, CURRENT_SCHEMA_VERSION + 10);
    }
}
