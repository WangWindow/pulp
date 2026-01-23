use iced::widget::svg;
use icondata::Icon;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// SVG Handle 缓存：减少重复构建与分配（性能优化）。
fn icon_handle_cache() -> &'static Mutex<HashMap<String, svg::Handle>> {
    static CACHE: OnceLock<Mutex<HashMap<String, svg::Handle>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn icon_handle(icon: Icon) -> svg::Handle {
    let view_box = icon.view_box.unwrap_or("0 0 24 24");
    let width = icon.width.unwrap_or("24");
    let height = icon.height.unwrap_or("24");
    // 注意：部分 SVG 渲染器对 `currentColor` 支持不完整，会导致图标完全不显示。
    // 我们统一使用黑色作为底色，再通过 iced 的 `svg::Style { color: Some(...) }`
    // 做主题着色（符号图标的推荐做法）。
    let fill = icon.fill.unwrap_or("#000000");
    let fill = if fill == "currentColor" {
        "#000000"
    } else {
        fill
    };

    // iced 的 SVG 颜色过滤（svg::Style.color）会对“符号图标”做统一着色。
    // 但部分 SVG 片段会使用 `currentColor` 作为 stroke/fill，这在某些渲染器下
    // 会解析失败，导致图标完全不显示。
    // 这里统一把 `currentColor` 替换为黑色，让图标至少能被正确绘制出来。
    let data = icon.data.replace("currentColor", "#000000");

    // 中文注释：缓存 key 使用最终 SVG 相关字段，避免重复构建 Handle。
    let cache_key = format!("{view_box}|{width}|{height}|{fill}|{data}");
    if let Ok(cache) = icon_handle_cache().lock() {
        if let Some(handle) = cache.get(&cache_key) {
            return handle.clone();
        }
    }

    // icondata 的 `data` 通常是 SVG 片段（一个或多个 `<path .../>`），但也有
    // 少数图标仅提供 path 的 d 值。两种情况都兼容。
    let body = if data.contains('<') {
        data
    } else {
        format!("<path fill=\"{fill}\" d=\"{data}\"/>")
    };

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"{view_box}\" fill=\"{fill}\">{body}</svg>"
    );
    let handle = svg::Handle::from_memory(svg.into_bytes());

    if let Ok(mut cache) = icon_handle_cache().lock() {
        cache.insert(cache_key, handle.clone());
    }

    handle
}
