//! UI i18n：语言选择与持久化模型

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Pulp UI 支持的语言集合。
///
/// 说明：
/// - rust-i18n 的 locale 字符串通常使用 BCP-47 风格（如 "zh-CN"）。
/// - 这里使用受控枚举，避免 UI/配置中出现任意字符串导致不可预期行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppLocale {
    En,
    ZhCn,
}

impl AppLocale {
    /// 返回用于 rust-i18n 的 locale 标识。
    pub fn as_str(self) -> &'static str {
        match self {
            AppLocale::En => "en",
            AppLocale::ZhCn => "zh-CN",
        }
    }

    /// 用户可读标签 key（交给 i18n 去翻译），避免硬编码语言名。
    ///
    /// 约定：UI 侧用 `t!(...)` 渲染这些 key。
    pub fn label_key(self) -> &'static str {
        match self {
            AppLocale::En => "menu.settings.language.english",
            AppLocale::ZhCn => "menu.settings.language.chinese_simplified",
        }
    }

    /// 从字符串解析（用于读取配置/CLI/调试）。
    ///
    /// 说明：
    /// - 允许常见写法：en、en-US、zh、zh-CN、zh_CN。
    /// - 对于 zh / zh-* 统一选择简体中文（ZhCn），后续如需繁体可扩展。
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        let norm = s.replace('_', "-").to_ascii_lowercase();

        // 英文分支：en 或 en-*
        if norm == "en" || norm.starts_with("en-") {
            return Some(AppLocale::En);
        }

        // 中文：zh 或 zh-*
        if norm == "zh" || norm.starts_with("zh-") {
            // 目前只提供 zh-CN；若未来加入 zh-TW/zh-HK，可在此细分。
            return Some(AppLocale::ZhCn);
        }

        None
    }
}

/// 语言偏好：
/// - FollowSystem：跟随系统（启动时检测系统 locale）
/// - Fixed：固定为某个语言（用户手动选择）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalePreference {
    FollowSystem,
    Fixed(AppLocale),
}

impl Default for LocalePreference {
    fn default() -> Self {
        LocalePreference::FollowSystem
    }
}

impl LocalePreference {
    /// 用户可读标签 key（供 UI 下拉框/设置页使用）。
    pub fn label_key(self) -> &'static str {
        match self {
            LocalePreference::FollowSystem => "menu.settings.language.follow_system",
            LocalePreference::Fixed(locale) => locale.label_key(),
        }
    }
}

/// 语言状态：用于 UI 运行时管理（便于实现“回滚到系统语言”）。
///
/// 说明：
/// - `preference`：用户选择的偏好（可持久化）。
/// - `effective`：根据偏好 + 系统检测最终应用的语言（用于 set_locale）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleState {
    pub preference: LocalePreference,
    pub effective: AppLocale,
}

impl LocaleState {
    /// 根据偏好和系统检测结果计算 effective locale。
    ///
    /// `system_locale`：来自系统的 locale 标识（例如 "en-US" / "zh-CN"）。
    /// 如为 None 或无法识别，则回退 `fallback`（通常为英文）。
    pub fn resolve(
        preference: LocalePreference,
        system_locale: Option<&str>,
        fallback: AppLocale,
    ) -> Self {
        let effective = match preference {
            LocalePreference::Fixed(locale) => locale,
            LocalePreference::FollowSystem => {
                system_locale.and_then(AppLocale::parse).unwrap_or(fallback)
            }
        };

        Self {
            preference,
            effective,
        }
    }

    /// 生成 rust-i18n 需要的 locale 字符串。
    pub fn effective_locale_str(&self) -> &'static str {
        self.effective.as_str()
    }
}

/// 持久化用的设置值（写入 config.toml）。
///
/// 设计目标：
/// - TOML 中用“人类可读字符串”存储，而不是序列化枚举的内部结构。
/// - 只允许：`"system"`、`"en"`、`"zh-CN"`（其余值会被解释为 system，保证可用性）。
///
/// 示例（TOML）：
/// - locale = "system"
/// - locale = "en"
/// - locale = "zh-CN"
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocaleSettingValue(
    /// 约束：只接受 system/en/zh-CN；其余值会在 to_preference() 中回退处理。
    pub Cow<'static, str>,
);

impl Default for LocaleSettingValue {
    fn default() -> Self {
        // 默认跟随系统，符合“支持设置与回滚”（回滚就是回到 system）。
        LocaleSettingValue(Cow::Borrowed("system"))
    }
}

impl LocaleSettingValue {
    /// 将偏好转换为可持久化值。
    pub fn from_preference(pref: LocalePreference) -> Self {
        match pref {
            LocalePreference::FollowSystem => LocaleSettingValue(Cow::Borrowed("system")),
            LocalePreference::Fixed(locale) => LocaleSettingValue(Cow::Borrowed(locale.as_str())),
        }
    }

    /// 将可持久化值转换为偏好。
    ///
    /// 解析失败时回退为 FollowSystem（配置损坏/用户手改配置时尽量保持可用）。
    pub fn to_preference(&self) -> LocalePreference {
        let raw = self.0.as_ref();
        if raw.eq_ignore_ascii_case("system") || raw.eq_ignore_ascii_case("auto") {
            return LocalePreference::FollowSystem;
        }

        AppLocale::parse(raw)
            .map(LocalePreference::Fixed)
            .unwrap_or(LocalePreference::FollowSystem)
    }
}

/// “系统语言”检测策略的轻量结果。
///
/// 上层可以使用 `sys-locale` 获取原始字符串后传入 `LocaleState::resolve(...)`。
///
/// 之所以单独提供此函数：
/// - 便于在 UI 里统一处理常见的 locale 垃圾输入（如空字符串/大小写/下划线）。
/// - 不把第三方 crate 的行为细节扩散到业务层。
pub fn normalize_system_locale(raw: Option<String>) -> Option<String> {
    let s = raw?.trim().to_string();
    if s.is_empty() {
        return None;
    }
    // sys-locale 可能返回 "zh_CN" 之类；统一转成 BCP-47 风格。
    Some(s.replace('_', "-"))
}
