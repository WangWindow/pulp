//! 虚拟列表：仅渲染可见区间，避免一次性构建大量行。

use iced::widget::scrollable::Viewport;
use iced::widget::{column, space};
use iced::{Element, Length};

#[derive(Debug, Clone, Copy)]
pub struct VirtualListConfig {
    pub row_height: f32,
    pub overscan: usize,
    pub viewport: Option<Viewport>,
}

impl VirtualListConfig {
    pub fn new(row_height: f32, overscan: usize, viewport: Option<Viewport>) -> Self {
        Self {
            row_height,
            overscan,
            viewport,
        }
    }
}

pub fn virtual_list<'a, T, M: 'a, F>(
    items: &'a [T],
    config: VirtualListConfig,
    render: F,
) -> Element<'a, M>
where
    F: Fn(&'a T) -> Element<'a, M> + 'a,
{
    if items.is_empty() {
        return column![].into();
    }

    let row_height = config.row_height.max(1.0);
    let total = items.len();

    let (mut start, mut end) = if let Some(viewport) = config.viewport {
        let offset = viewport.absolute_offset().y.max(0.0);
        let height = viewport.bounds().height.max(1.0);

        let start = (offset / row_height).floor() as usize;
        let end = ((offset + height) / row_height).ceil() as usize;
        (start, end.min(total))
    } else {
        (0, total.min(60))
    };

    start = start.saturating_sub(config.overscan);
    end = (end + config.overscan).min(total);

    let top_pad = start as f32 * row_height;
    let bottom_pad = (total - end) as f32 * row_height;

    let mut col = column![].spacing(0).width(Length::Fill);

    if top_pad > 0.0 {
        col = col.push(space().height(Length::Fixed(top_pad)));
    }

    let render_ref = &render;
    for item in &items[start..end] {
        col = col.push(render_ref(item));
    }

    if bottom_pad > 0.0 {
        col = col.push(space().height(Length::Fixed(bottom_pad)));
    }

    col.into()
}
