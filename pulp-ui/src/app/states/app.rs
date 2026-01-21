//! App 状态结构与基础构造。

use crate::app::config;
use crate::domain::{
    DRAWER_DEFAULT_WIDTH_PX, DrawerPanel, EntryRow, FileEntry, Message, Page, ThemeMode, ViewMode,
};
use iced::widget::scrollable::Viewport;
use iced::{Subscription, Task, Theme};
use pulp_core::CancellationToken;
use rust_i18n::t;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

/// iced 应用的全部状态。
///
/// 说明：
/// - UI 只负责渲染与交互；复杂任务交由 core 处理；
/// - 这里保留“任务状态/抽屉状态”等 UI 运行时数据。
pub struct App {
    pub(super) path: String,
    pub(super) filter: String,
    pub(super) selected_path: PathBuf,
    pub(super) page: Page,
    pub(super) view_mode: ViewMode,
    pub(super) back_stack: Vec<PathBuf>,
    pub(super) forward_stack: Vec<PathBuf>,

    // -------------------------------
    // 文件系统数据（原始）与渲染数据（统一行模型）
    // -------------------------------
    pub(super) all_entries: Vec<FileEntry>,
    pub(super) entries: Vec<FileEntry>,
    pub(super) rendered_rows: Vec<EntryRow>,

    // 压缩包原始条目（来自 pulp-core）
    pub(super) archive_entries: Vec<pulp_core::ArchiveEntry>,
    // 压缩包预览的统一渲染行（虚拟路径：`archive.zip::/src/main.rs`）
    pub(super) archive_rendered_rows: Vec<EntryRow>,
    pub(super) active_archive: Option<PathBuf>,
    pub(super) status: String,
    pub(super) menu_open: bool,
    pub(super) title_menu_open: bool,
    pub(super) location_editing: bool,
    pub(super) theme_mode: ThemeMode,
    pub(super) system_dark: bool,
    pub(super) selected_entry: Option<PathBuf>,
    pub(super) extract_to_target: Option<PathBuf>,
    pub(super) extract_to_path: String,
    pub(super) new_folder_open: bool,
    pub(super) new_folder_name: String,
    pub(super) rename_target: Option<PathBuf>,
    pub(super) rename_name: String,
    pub(super) delete_target: Option<PathBuf>,
    pub(super) last_click: Option<(PathBuf, std::time::Instant)>,
    pub(super) spinner_index: usize,
    pub(super) busy: bool,
    pub(super) pending_extract: bool,

    // -------------------------------
    // 虚拟列表：滚动视口信息（用于计算可见行）
    // -------------------------------
    pub(super) list_viewport: Option<Viewport>,

    // -------------------------------
    // 任务队列（控制异步并发）
    // -------------------------------
    pub(super) task_queue: VecDeque<super::tasks::UiTask>,
    pub(super) task_running: bool,

    // -------------------------------
    // 树状展开列表（平铺/树状统一）UI 模型
    // -------------------------------
    /// 已展开的目录集合（key => expanded）。
    pub(super) tree_expanded: HashSet<PathBuf>,
    /// 文件系统：子目录缓存（真实目录 path -> children EntryRow）。
    pub(super) fs_children_cache: HashMap<PathBuf, Vec<EntryRow>>,
    /// 压缩包：虚拟目录索引（虚拟目录 key -> children EntryRow）。
    pub(super) archive_children_index: HashMap<PathBuf, Vec<EntryRow>>,
    /// 当前树状视图要渲染的“扁平行”列表（已经写入 depth）。
    pub(super) tree_render_rows: Vec<EntryRow>,
    /// 文件系统：正在加载 children 的目录（避免重复触发 IO）。
    pub(super) fs_tree_loading: HashSet<PathBuf>,

    // 单任务模型：Drawer::Task 用于展示细粒度进度、当前文件、取消、结果。
    pub(super) active_task_title: Option<String>,
    pub(super) active_task_current_entry: Option<String>,
    pub(super) active_task_progress: Option<f32>,
    pub(super) active_task_finished: bool,
    pub(super) active_task_cancelled: bool,
    pub(super) active_task_error: Option<String>,
    pub(super) active_task_cancel: Option<CancellationToken>,

    // Drawer（右侧抽屉）：v1 先把“对话框/任务面板”迁移到这里（单窗口）
    pub(super) drawer_open: bool,
    pub(super) drawer_panel: DrawerPanel,
    pub(super) drawer_width_px: f32,
    pub(super) drawer_resizing: bool,

    // Drawer resize drag state（全局鼠标事件驱动）
    pub(super) drawer_resize_last_cursor_x: Option<f32>,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let root_path = PathBuf::from(home);

        // 默认跟随系统亮暗。
        let system_dark = dark_light::detect()
            .map(|m| matches!(m, dark_light::Mode::Dark))
            .unwrap_or(true);

        // i18n 启动策略：读取配置并根据系统 locale 应用。
        let cfg = config::load().unwrap_or_default();
        let preference = cfg.locale_preference();
        let system_locale = crate::i18n::normalize_system_locale(sys_locale::get_locale());
        let locale_state = crate::i18n::LocaleState::resolve(
            preference,
            system_locale.as_deref(),
            crate::i18n::AppLocale::En,
        );
        rust_i18n::set_locale(locale_state.effective_locale_str());

        (
            Self {
                path: root_path.display().to_string(),
                filter: String::new(),
                selected_path: root_path.clone(),
                page: Page::Browser,
                view_mode: ViewMode::FileSystem,
                back_stack: Vec::new(),
                forward_stack: Vec::new(),
                all_entries: Vec::new(),
                entries: Vec::new(),
                rendered_rows: Vec::new(),
                archive_entries: Vec::new(),
                archive_rendered_rows: Vec::new(),
                active_archive: None,
                status: t!("status.ready").to_string(),
                menu_open: true,
                title_menu_open: false,
                location_editing: false,
                theme_mode: ThemeMode::System,
                system_dark,
                selected_entry: None,
                extract_to_target: None,
                extract_to_path: String::new(),
                new_folder_open: false,
                new_folder_name: t!("status.default_new_folder_name").to_string(),
                rename_target: None,
                rename_name: String::new(),
                delete_target: None,
                last_click: None,
                spinner_index: 0,
                busy: false,
                pending_extract: false,

                list_viewport: None,

                task_queue: VecDeque::new(),
                task_running: false,

                tree_expanded: HashSet::new(),
                fs_children_cache: HashMap::new(),
                archive_children_index: HashMap::new(),
                tree_render_rows: Vec::new(),
                fs_tree_loading: HashSet::new(),

                active_task_title: None,
                active_task_current_entry: None,
                active_task_progress: None,
                active_task_finished: false,
                active_task_cancelled: false,
                active_task_error: None,
                active_task_cancel: None,

                drawer_open: false,
                drawer_panel: DrawerPanel::Task,
                drawer_width_px: DRAWER_DEFAULT_WIDTH_PX,
                drawer_resizing: false,

                drawer_resize_last_cursor_x: None,
            },
            Task::perform(
                crate::utils::fs::load_directory(root_path),
                |(path, entries)| Message::DirLoaded(path, entries),
            ),
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![iced::event::listen().map(Message::Event)];
        if self.busy {
            subs.push(iced::time::every(Duration::from_millis(140)).map(|_| Message::Tick));
        }
        Subscription::batch(subs)
    }

    pub fn theme(&self) -> Theme {
        match self.theme_mode {
            ThemeMode::Light => Theme::Light,
            ThemeMode::Dark => Theme::Dark,
            ThemeMode::System => {
                if self.system_dark {
                    Theme::Dark
                } else {
                    Theme::Light
                }
            }
        }
    }
}
