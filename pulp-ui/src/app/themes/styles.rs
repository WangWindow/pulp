use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Theme};

fn colors(theme: &Theme) -> catppuccin::FlavorColors {
    // 约定：Theme::Light => Latte，其它 => Macchiato（深色）。
    // 这样 App::theme() 只要返回 Light/Dark，就能全局切换配色。
    match theme {
        Theme::Light => catppuccin::PALETTE.latte.colors,
        _ => catppuccin::PALETTE.macchiato.colors,
    }
}

fn to_color(color: catppuccin::Color) -> Color {
    Color::from_rgb8(color.rgb.r, color.rgb.g, color.rgb.b)
}

/// Drawer（右侧抽屉）分割线：默认可见，hover 更亮/略加粗。
pub fn drawer_divider_style(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(Background::Color(to_color(c.surface0))),
        text_color: None,
        border: Border {
            radius: 0.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// Drawer（右侧抽屉）分割线的 hover/active 状态（拖拽时也用这个）。
pub fn drawer_divider_active_style(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(Background::Color(to_color(c.surface1))),
        text_color: None,
        border: Border {
            radius: 0.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// SVG 图标的推荐颜色（跟随主题文字色）。
pub fn icon_color(theme: &Theme) -> Color {
    let c = colors(theme);
    to_color(c.text)
}

pub fn app_style(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(Background::Color(to_color(c.base))),
        text_color: Some(to_color(c.text)),
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: to_color(c.surface0),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

pub fn panel_style(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(Background::Color(to_color(c.mantle))),
        text_color: Some(to_color(c.text)),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: to_color(c.surface0),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// 菜单面板样式：更明显的边界与阴影。
pub fn menu_panel_style(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(Background::Color(to_color(c.surface1))),
        text_color: Some(to_color(c.text)),
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: to_color(c.surface1),
        },
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.2),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
        snap: false,
    }
}

/// 右键菜单专用面板样式：更强的浮层感和分割线色彩。
pub fn context_menu_panel_style(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(Background::Color(to_color(c.base))),
        text_color: Some(to_color(c.text)),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: to_color(c.surface2),
        },
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.28),
            offset: iced::Vector::new(0.0, 10.0),
            blur_radius: 24.0,
        },
        snap: false,
    }
}

/// 菜单条目样式
pub fn menu_item_style(_theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(_theme);
    let mut style = button::Style::default();

    match status {
        button::Status::Active => {
            style.background = Some(Background::Color(to_color(c.surface1)));
            style.text_color = to_color(c.text);
        }
        button::Status::Hovered => {
            style.background = Some(Background::Color(to_color(c.surface2)));
            style.text_color = to_color(c.text);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(to_color(c.surface2)));
            style.text_color = to_color(c.text);
        }
        button::Status::Disabled => {
            style.background = None;
            style.text_color = to_color(c.overlay0);
        }
    }

    style.border = Border {
        radius: 6.0.into(),
        width: 0.0,
        color: Color::TRANSPARENT,
    };
    style
}

/// 顶部 AppBar 的背景风格（更亮一层，突出层级）。
pub fn appbar_style(_theme: &Theme) -> container::Style {
    let c = colors(_theme);
    container::Style {
        background: Some(Background::Color(to_color(c.surface0))),
        text_color: Some(to_color(c.text)),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: to_color(c.surface1),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// 适用于图标按钮/幽灵按钮：默认透明，hover 有底色。
pub fn ghost_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(_theme);
    let mut style = button::Style::default();

    match status {
        button::Status::Active => {
            style.background = None;
            style.text_color = to_color(c.text);
        }
        button::Status::Hovered => {
            style.background = Some(Background::Color(to_color(c.surface1)));
            style.text_color = to_color(c.text);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(to_color(c.surface2)));
            style.text_color = to_color(c.text);
        }
        button::Status::Disabled => {
            style.background = None;
            style.text_color = to_color(c.overlay0);
        }
    }

    style.border = Border {
        radius: 8.0.into(),
        width: 1.0,
        color: Color::TRANSPARENT,
    };
    style
}

/// 列表/树状行的选中样式：选中态更突出，hover 仍保留。
pub fn list_row_style(_theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let c = colors(_theme);
    let mut style = button::Style::default();

    let base_bg = if selected {
        to_color(c.surface1)
    } else {
        to_color(c.surface0)
    };
    let hover_bg = if selected {
        to_color(c.surface2)
    } else {
        to_color(c.surface1)
    };
    let pressed_bg = to_color(c.surface2);

    match status {
        button::Status::Active => {
            style.background = if selected {
                Some(Background::Color(base_bg))
            } else {
                None
            };
            style.text_color = to_color(c.text);
        }
        button::Status::Hovered => {
            style.background = Some(Background::Color(hover_bg));
            style.text_color = to_color(c.text);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(pressed_bg));
            style.text_color = to_color(c.text);
        }
        button::Status::Disabled => {
            style.background = None;
            style.text_color = to_color(c.overlay0);
        }
    }

    style.border = Border {
        radius: 8.0.into(),
        width: if selected { 1.0 } else { 0.0 },
        color: if selected {
            to_color(c.surface2)
        } else {
            Color::TRANSPARENT
        },
    };

    style
}
