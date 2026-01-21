//! CLI 国际化（i18n）辅助模块。
//!
//! 设计目标（对调用方有用的不变量）：
//! - 默认语言：跟随系统语言；若无法识别/不支持，则回退到英文（fallback=en 由 rust-i18n 初始化保证）。
//! - 允许通过 CLI 参数显式指定语言：`en` / `zh-CN` / `system`。
//! - 支持“回滚”：当用户传入 `system` 时，恢复为跟随系统语言（再次检测并设置）。

/// Pulp CLI 支持的语言枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliLocale {
    /// 跟随系统语言（运行时检测）。
    System,
    /// 英文（固定）。
    En,
    /// 简体中文（固定：zh-CN）。
    ZhCn,
}

/// 解析用户从 CLI 传入的 locale 字符串。
///
/// 支持的输入（大小写不敏感，允许部分常见别名）：
/// - `"system"` / `"auto"`：跟随系统
/// - `"en"` / `"en-us"` / `"en_us"`：英文
/// - `"zh"` / `"zh-cn"` / `"zh_cn"` / `"zh-hans"`：简体中文（统一映射到 zh-CN）
///
/// 返回值：
/// - `Some(CliLocale)`：解析成功
/// - `None`：无法识别；调用方可选择报错或忽略并使用默认策略（system -> en fallback）。
pub fn parse_locale_arg(s: &str) -> Option<CliLocale> {
    let norm = normalize_locale_tag(s);

    match norm.as_str() {
        "system" | "auto" => Some(CliLocale::System),
        "en" | "en-us" => Some(CliLocale::En),
        "zh" | "zh-cn" | "zh-hans" => Some(CliLocale::ZhCn),
        _ => None,
    }
}

/// 应用 locale 选择到 rust-i18n 全局状态。
///
/// 规则：
/// 1. 如果用户显式指定 `En` 或 `ZhCn`：直接设置。
/// 2. 如果是 `System`：
///    - 读取系统 locale；
///    - 归一化后只映射到我们支持的 `en` / `zh-CN`；
///    - 若无法识别或不在支持集合中，则设为 `en`。
///
/// 返回值：
/// - 实际生效的 locale 字符串（只可能是 `"en"` 或 `"zh-CN"`）。
pub fn apply_locale(choice: CliLocale) -> &'static str {
    let selected = match choice {
        CliLocale::En => "en",
        CliLocale::ZhCn => "zh-CN",
        CliLocale::System => select_system_locale_supported(),
    };

    // rust-i18n 在 main.rs 中已配置 fallback="en"，但这里仍显式 set，
    // 以免调用方依赖隐式行为。
    rust_i18n::set_locale(selected);
    selected
}

/// 从系统环境中选择我们支持的 locale（只返回 en 或 zh-CN）。
fn select_system_locale_supported() -> &'static str {
    // sys-locale 会返回类似：en_US / zh_CN / zh-CN / en-US 等。
    let Some(raw) = sys_locale::get_locale() else {
        return "en";
    };

    let norm = normalize_locale_tag(&raw);

    // 只支持 en 与 zh-CN 两种资源：其余一律回退 en。
    if norm == "zh" || norm == "zh-cn" || norm == "zh-hans" {
        "zh-CN"
    } else if norm == "en" || norm == "en-us" {
        "en"
    } else {
        // 处理 rust-i18n 的“语言-地区回退链”特性：
        // 例如系统是 "zh-HK"，我们没有 zh-HK，但有 zh-CN。
        // 这里按“中文=zh* -> zh-CN”做一个合理映射。
        if norm.starts_with("zh") {
            "zh-CN"
        } else {
            "en"
        }
    }
}

/// 将各种 locale 表达归一化到更容易匹配的形式。
///
/// 规则：
/// - Trim + 转小写
/// - 下划线转为短横线（en_US -> en-us）
/// - 去掉多余空白
fn normalize_locale_tag(input: &str) -> String {
    input.trim().replace('_', "-").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_locale_arg_variants() {
        assert_eq!(parse_locale_arg("system"), Some(CliLocale::System));
        assert_eq!(parse_locale_arg("AUTO"), Some(CliLocale::System));

        assert_eq!(parse_locale_arg("en"), Some(CliLocale::En));
        assert_eq!(parse_locale_arg("en-US"), Some(CliLocale::En));
        assert_eq!(parse_locale_arg("en_us"), Some(CliLocale::En));

        assert_eq!(parse_locale_arg("zh"), Some(CliLocale::ZhCn));
        assert_eq!(parse_locale_arg("zh-CN"), Some(CliLocale::ZhCn));
        assert_eq!(parse_locale_arg("zh_cn"), Some(CliLocale::ZhCn));
        assert_eq!(parse_locale_arg("zh-Hans"), Some(CliLocale::ZhCn));

        assert_eq!(parse_locale_arg("fr"), None);
    }

    #[test]
    fn normalize_locale_tag_basic() {
        assert_eq!(normalize_locale_tag(" en_US "), "en-us");
        assert_eq!(normalize_locale_tag("zh-CN"), "zh-cn");
    }
}
