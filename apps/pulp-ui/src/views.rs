//! Render-only application views.

mod archive;
mod settings;

pub use archive::render as render_archive;
pub use settings::render as render_settings;
