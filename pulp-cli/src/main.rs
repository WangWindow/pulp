mod i18n;

use clap::{Args, Parser, Subcommand, ValueEnum};
use pulp_core::{
    ArchiveFormat, ArchiveService, ArchiveSource, CancellationToken, CompressOptions,
    DefaultArchiveService, ExtractOptions, ListOptions, ProgressEvent, ProgressReporter, TaskId,
    create_default_service,
};
use rust_i18n::t;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

rust_i18n::i18n!("locales", fallback = "en");

/// pulp-cli：多任务命令行入口。
#[derive(Debug, Parser)]
#[command(
    name = "pulp",
    version,
    about = "A pure-Rust-first archive tool (compress/extract/list)."
)]
struct Cli {
    /// 语言/区域：en | zh-CN | system（跟随系统）
    #[arg(long, default_value = "system")]
    locale: String,

    /// 刷新间隔（毫秒）：用于打印聚合进度。
    #[arg(long, default_value_t = 200)]
    refresh_ms: u64,

    /// 并发任务数限制（0 表示不限制）。
    ///
    /// 说明：仅对本 CLI 自己的 “spawn 任务数量” 做限制；不影响 core 内部实现。
    #[arg(long, default_value_t = 0)]
    concurrency: usize,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 列出一个或多个压缩包条目（可并行）
    List(ListCmd),

    /// 解压一个或多个压缩包（可并行）
    Extract(ExtractCmd),

    /// 压缩（可并行）
    Compress(CompressCmd),

    /// 输出当前 core 支持的格式列表（用于排错）
    Probe,
}

#[derive(Debug, Args)]
struct ListCmd {
    /// 压缩包路径（可传多个并并行 list）
    #[arg(required = true, num_args = 1..)]
    archives: Vec<PathBuf>,

    /// 密码（如需）
    #[arg(long)]
    password: Option<String>,
}

#[derive(Debug, Args)]
struct ExtractCmd {
    /// 压缩包路径（可传多个并并行 extract）
    #[arg(required = true, num_args = 1..)]
    archives: Vec<PathBuf>,

    /// 输出目录（多个 archive 会解压到该目录下的同名子目录，避免互相覆盖）
    #[arg(long)]
    dest_dir: PathBuf,

    /// 覆盖已存在文件
    #[arg(long)]
    overwrite: bool,

    /// 平铺输出（不保留路径结构）
    #[arg(long)]
    flat: bool,

    /// 剥离前 N 级路径
    #[arg(long = "strip-components")]
    strip_components: Option<usize>,

    /// 密码（如需）
    #[arg(long)]
    password: Option<String>,

    /// 线程数提示（后端可选）
    #[arg(long)]
    threads: Option<usize>,
}

#[derive(Debug, Args)]
struct CompressCmd {
    /// 输出压缩包路径（可传多个；与 inputs 一一对应）
    ///
    /// 规则：
    /// - 当 --out 只有 1 个时：把所有 inputs 打进同一个压缩包。
    /// - 当 --out 多个时：要求 inputs 数量一致，逐个打包。
    #[arg(long = "out", required = true, num_args = 1..)]
    dest_archives: Vec<PathBuf>,

    /// 输入文件/目录
    #[arg(required = true, num_args = 1..)]
    inputs: Vec<PathBuf>,

    /// 指定格式（默认从输出扩展名推断）
    #[arg(long = "format")]
    format_hint: Option<FormatHint>,

    /// 压缩等级（语义由后端决定，通常 0-9）
    #[arg(long)]
    level: Option<u8>,

    /// 启用 solid（仅部分格式有效）
    #[arg(long)]
    solid: bool,

    /// 密码（如需）
    #[arg(long)]
    password: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum FormatHint {
    Zip,
    SevenZ,
    Rar,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
}

impl From<FormatHint> for ArchiveFormat {
    fn from(value: FormatHint) -> Self {
        match value {
            FormatHint::Zip => ArchiveFormat::Zip,
            FormatHint::SevenZ => ArchiveFormat::SevenZ,
            FormatHint::Rar => ArchiveFormat::Rar,
            FormatHint::Tar => ArchiveFormat::Tar,
            FormatHint::TarGz => ArchiveFormat::TarGz,
            FormatHint::TarBz2 => ArchiveFormat::TarBz2,
            FormatHint::TarXz => ArchiveFormat::TarXz,
        }
    }
}

#[derive(Debug, Clone)]
struct TaskSnapshot {
    kind: String,
    title: String,
    backend: Option<String>,
    phase: Option<String>,
    current_entry: Option<String>,
    processed: u64,
    total: Option<u64>,
    finished: bool,
    cancelled: bool,
    error: Option<String>,
    warnings: Vec<String>,
    last_event_at: Instant,
}

impl TaskSnapshot {
    fn new(kind: String, title: String) -> Self {
        Self {
            kind,
            title,
            backend: None,
            phase: None,
            current_entry: None,
            processed: 0,
            total: None,
            finished: false,
            cancelled: false,
            error: None,
            warnings: Vec::new(),
            last_event_at: Instant::now(),
        }
    }
}

#[derive(Debug, Default)]
struct TaskDashboard {
    tasks: HashMap<TaskId, TaskSnapshot>,
}

impl TaskDashboard {
    fn ensure(&mut self, id: TaskId, kind: String, title: String) -> &mut TaskSnapshot {
        self.tasks
            .entry(id)
            .or_insert_with(|| TaskSnapshot::new(kind, title))
    }

    fn render_compact(&self) -> String {
        // 说明：CLI 采用“追加打印”的方式而非清屏，减少终端兼容问题。
        // 你如果需要更像 TUI 的体验，可以后续用 crossterm + alternate screen 实现。
        let mut ids: Vec<_> = self.tasks.keys().copied().collect();
        ids.sort_by_key(|id| *id);

        let mut out = String::new();
        out.push_str("----\n");

        for id in ids {
            let t = &self.tasks[&id];

            let status = if t.cancelled {
                t!("task.title.cancelled").to_string()
            } else if t.error.is_some() {
                t!("task.title.failed").to_string()
            } else if t.finished {
                t!("task.title.finished").to_string()
            } else {
                t!("task.title.running").to_string()
            };

            let backend = t.backend.as_deref().unwrap_or("-");

            let phase = t.phase.as_deref().unwrap_or("-");

            let entry = t.current_entry.as_deref().unwrap_or("-");

            let progress = match t.total {
                Some(total) => format!("{}/{}", t.processed, total),
                None => format!("{}", t.processed),
            };

            out.push_str(&format!(
                "[#{:>3}] {status} | {} | {} | phase={phase} | backend={backend} | {progress} | {entry}\n",
                id,
                t.kind,
                t.title
            ));

            // warnings：保持简短，避免刷屏
            for w in t.warnings.iter().take(3) {
                out.push_str(&format!(
                    "[#{:>3}] {}\n",
                    id,
                    t!("task.warning", message = w)
                ));
            }
            if t.warnings.len() > 3 {
                out.push_str(&format!(
                    "[#{:>3}] {} (+{})\n",
                    id,
                    t!("task.warning", message = "…"),
                    t.warnings.len() - 3
                ));
            }

            if let Some(e) = &t.error {
                out.push_str(&format!(
                    "[#{:>3}] {}\n",
                    id,
                    t!("task.error", message = e.as_str())
                ));
            }
        }

        out
    }
}

struct DashboardReporter {
    dashboard: Arc<Mutex<TaskDashboard>>,
}

impl DashboardReporter {
    fn new(dashboard: Arc<Mutex<TaskDashboard>>) -> Self {
        Self { dashboard }
    }
}

impl ProgressReporter for DashboardReporter {
    fn report(&self, event: ProgressEvent) {
        let mut dash = self.dashboard.lock().expect("dashboard mutex poisoned");

        match event {
            ProgressEvent::TaskStarted {
                task_id,
                kind,
                title,
            } => {
                dash.ensure(task_id, format!("{kind:?}"), title)
                    .last_event_at = Instant::now();
            }
            ProgressEvent::BackendSelected { task_id, backend } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.backend = Some(backend);
                t.last_event_at = Instant::now();
            }
            ProgressEvent::PhaseChanged { task_id, phase } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.phase = Some(format!("{phase:?}"));
                t.last_event_at = Instant::now();
            }
            ProgressEvent::EntryStarted { task_id, entry } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.current_entry = Some(entry.path.display().to_string());
                t.processed = entry.processed_bytes;
                t.total = entry.total_bytes;
                t.last_event_at = Instant::now();
            }
            ProgressEvent::EntryProgress { task_id, entry } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.current_entry = Some(entry.path.display().to_string());
                t.processed = entry.processed_bytes;
                t.total = entry.total_bytes.or(t.total);
                t.last_event_at = Instant::now();
            }
            ProgressEvent::EntryFinished { task_id, entry } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.current_entry = Some(entry.path.display().to_string());
                t.last_event_at = Instant::now();
            }
            ProgressEvent::Warning { task_id, message } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.warnings.push(message);
                t.last_event_at = Instant::now();
            }
            ProgressEvent::Note { task_id, message } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                // note 作为 warning 的轻量替代（避免新增字段导致输出太复杂）
                t.warnings.push(message);
                t.last_event_at = Instant::now();
            }
            ProgressEvent::TaskFinished { task_id } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.finished = true;
                t.last_event_at = Instant::now();
            }
            ProgressEvent::TaskCancelled { task_id } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.cancelled = true;
                t.last_event_at = Instant::now();
            }
            ProgressEvent::TaskFailed { task_id, message } => {
                let t = dash.ensure(task_id, "-".into(), "-".into());
                t.error = Some(message);
                t.last_event_at = Instant::now();
            }
        }
    }
}

static TASK_SEQ: AtomicU64 = AtomicU64::new(1);

fn next_task_id() -> TaskId {
    TASK_SEQ.fetch_add(1, Ordering::Relaxed)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // 应用 i18n locale（system/en/zh-CN）。
    let locale_choice = i18n::parse_locale_arg(&cli.locale).unwrap_or(i18n::CliLocale::System);
    i18n::apply_locale(locale_choice);

    let refresh = Duration::from_millis(cli.refresh_ms.max(50));
    let service = create_default_service();

    match cli.command {
        Commands::Probe => {
            let formats = service.supported_formats();
            println!("{}:", t!("opt.format"));
            for f in formats {
                println!("- {f}");
            }
            println!("{}", t!("out.done"));
        }

        Commands::List(cmd) => {
            let dashboard: Arc<Mutex<TaskDashboard>> =
                Arc::new(Mutex::new(TaskDashboard::default()));
            let reporter: Arc<dyn ProgressReporter + Send + Sync + 'static> =
                Arc::new(DashboardReporter::new(dashboard.clone()));
            let cancel_all = CancellationToken::new();

            let handles =
                spawn_list_jobs(service, cmd, reporter, cancel_all, cli.concurrency).await;

            run_dashboard_until_done(dashboard, refresh, &handles).await;
            exit_by_join_results(handles).await;
        }

        Commands::Extract(cmd) => {
            let dashboard: Arc<Mutex<TaskDashboard>> =
                Arc::new(Mutex::new(TaskDashboard::default()));
            let reporter: Arc<dyn ProgressReporter + Send + Sync + 'static> =
                Arc::new(DashboardReporter::new(dashboard.clone()));
            let cancel_all = CancellationToken::new();

            let handles =
                spawn_extract_jobs(service, cmd, reporter, cancel_all, cli.concurrency).await;

            run_dashboard_until_done(dashboard, refresh, &handles).await;
            exit_by_join_results(handles).await;
        }

        Commands::Compress(cmd) => {
            let dashboard: Arc<Mutex<TaskDashboard>> =
                Arc::new(Mutex::new(TaskDashboard::default()));
            let reporter: Arc<dyn ProgressReporter + Send + Sync + 'static> =
                Arc::new(DashboardReporter::new(dashboard.clone()));
            let cancel_all = CancellationToken::new();

            let handles =
                spawn_compress_jobs(service, cmd, reporter, cancel_all, cli.concurrency).await;

            run_dashboard_until_done(dashboard, refresh, &handles).await;
            exit_by_join_results(handles).await;
        }
    }
}

async fn spawn_list_jobs(
    service: DefaultArchiveService,
    cmd: ListCmd,
    reporter: Arc<dyn ProgressReporter + Send + Sync + 'static>,
    cancel_all: CancellationToken,
    concurrency: usize,
) -> Vec<tokio::task::JoinHandle<Result<(), String>>> {
    let sem = concurrency_to_semaphore(concurrency);

    let mut handles = Vec::new();
    for archive in cmd.archives {
        let permit = sem.clone().acquire_owned().await.ok();

        let service = service.clone();
        let reporter = reporter.clone();
        let cancel_all = cancel_all.clone();
        let password = cmd.password.clone();

        let task_id = next_task_id();
        let title = format!("list: {}", archive.display());

        handles.push(tokio::spawn(async move {
            // permit：drop 时释放
            let _permit = permit;

            let cancel = cancel_all;
            let source = ArchiveSource::new(archive.clone());
            let options = ListOptions { password };

            let entries = service
                .list(task_id, title, source, options, reporter.as_ref(), &cancel)
                .await
                .map_err(|e| e.to_string())?;

            println!("{}", t!("out.list.header"));
            for e in entries {
                if e.is_dir {
                    println!("{}", t!("out.list.item_dir", path = e.path));
                } else {
                    println!("{}", t!("out.list.item_file", path = e.path));
                }
            }

            Ok(())
        }));
    }

    handles
}

async fn spawn_extract_jobs(
    service: DefaultArchiveService,
    cmd: ExtractCmd,
    reporter: Arc<dyn ProgressReporter + Send + Sync + 'static>,
    cancel_all: CancellationToken,
    concurrency: usize,
) -> Vec<tokio::task::JoinHandle<Result<(), String>>> {
    let sem = concurrency_to_semaphore(concurrency);

    let mut handles = Vec::new();
    for archive in cmd.archives {
        let permit = sem.clone().acquire_owned().await.ok();

        let service = service.clone();
        let reporter = reporter.clone();
        let cancel_all = cancel_all.clone();

        let task_id = next_task_id();
        let title = format!("extract: {}", archive.display());

        // 每个 archive 解压到独立子目录，避免覆盖
        let base = cmd.dest_dir.clone();
        let subdir = archive
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("archive-{}", task_id));
        let dest_dir = base.join(subdir);

        let options = ExtractOptions {
            overwrite: cmd.overwrite,
            preserve_paths: !cmd.flat,
            strip_components: cmd.strip_components,
            password: cmd.password.clone(),
            threads: cmd.threads,
        };
        let source = ArchiveSource::new(archive.clone());

        handles.push(tokio::spawn(async move {
            let _permit = permit;

            eprintln!(
                "{}",
                t!("out.extract.start", archive = archive.display().to_string())
            );
            eprintln!(
                "{}",
                t!("out.extract.dest", dir = dest_dir.display().to_string())
            );

            let cancel = cancel_all;

            service
                .extract(
                    task_id,
                    title,
                    source,
                    dest_dir,
                    options,
                    reporter.as_ref(),
                    &cancel,
                )
                .await
                .map_err(|e| e.to_string())?;

            Ok(())
        }));
    }

    handles
}

async fn spawn_compress_jobs(
    service: DefaultArchiveService,
    cmd: CompressCmd,
    reporter: Arc<dyn ProgressReporter + Send + Sync + 'static>,
    cancel_all: CancellationToken,
    concurrency: usize,
) -> Vec<tokio::task::JoinHandle<Result<(), String>>> {
    let sem = concurrency_to_semaphore(concurrency);

    // job 拆分规则：
    // - 1 个输出：把所有 inputs 打到一个压缩包
    // - N 个输出：inputs 也必须 N 个，逐个压缩
    let mut jobs: Vec<(Vec<PathBuf>, PathBuf)> = Vec::new();
    if cmd.dest_archives.len() == 1 {
        jobs.push((cmd.inputs.clone(), cmd.dest_archives[0].clone()));
    } else {
        if cmd.dest_archives.len() != cmd.inputs.len() {
            let msg = "When multiple --out are provided, number of inputs must match.";
            return vec![tokio::spawn(async move { Err(msg.to_string()) })];
        }
        for (input, out) in cmd.inputs.into_iter().zip(cmd.dest_archives.into_iter()) {
            jobs.push((vec![input], out));
        }
    }

    let mut handles = Vec::new();
    for (inputs, dest_archive) in jobs {
        let permit = sem.clone().acquire_owned().await.ok();

        let service = service.clone();
        let reporter = reporter.clone();
        let cancel_all = cancel_all.clone();

        let task_id = next_task_id();
        let title = format!("compress: {}", dest_archive.display());

        let format = cmd
            .format_hint
            .map(ArchiveFormat::from)
            .or_else(|| ArchiveFormat::from_path(&dest_archive));

        if matches!(format, Some(ArchiveFormat::Rar)) {
            handles.push(tokio::spawn(async move {
                Err("RAR 暂不支持创建（仅支持解压/预览）".to_string())
            }));
            continue;
        }

        let options = CompressOptions {
            level: cmd.level,
            solid: cmd.solid.then_some(true),
            password: cmd.password.clone(),
        };

        handles.push(tokio::spawn(async move {
            let _permit = permit;

            eprintln!(
                "{}",
                t!(
                    "out.compress.start",
                    archive = dest_archive.display().to_string()
                )
            );

            let cancel = cancel_all;

            service
                .compress(
                    task_id,
                    title,
                    inputs,
                    dest_archive,
                    format,
                    options,
                    reporter.as_ref(),
                    &cancel,
                )
                .await
                .map_err(|e| e.to_string())?;

            Ok(())
        }));
    }

    handles
}

fn concurrency_to_semaphore(concurrency: usize) -> Arc<tokio::sync::Semaphore> {
    // 说明：
    // - concurrency=0 表示不限制，但 Semaphore 需要一个正数。
    // - 这里用一个较大的上限（1024）即可。
    let limit = if concurrency == 0 {
        1024
    } else {
        concurrency.max(1)
    };
    Arc::new(tokio::sync::Semaphore::new(limit))
}

async fn run_dashboard_until_done<T>(
    dashboard: Arc<Mutex<TaskDashboard>>,
    refresh: Duration,
    handles: &[tokio::task::JoinHandle<T>],
) {
    while handles.iter().any(|h| !h.is_finished()) {
        {
            let dash = dashboard.lock().expect("dashboard mutex poisoned");
            eprint!("{}", dash.render_compact());
        }
        tokio::time::sleep(refresh).await;
    }

    {
        let dash = dashboard.lock().expect("dashboard mutex poisoned");
        eprint!("{}", dash.render_compact());
    }
}

async fn exit_by_join_results(handles: Vec<tokio::task::JoinHandle<Result<(), String>>>) -> ! {
    let mut had_error = false;

    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(msg)) => {
                had_error = true;
                eprintln!("{}", t!("err.generic", message = msg.as_str()));
            }
            Err(join_err) => {
                had_error = true;
                eprintln!(
                    "{}",
                    t!("err.generic", message = join_err.to_string().as_str())
                );
            }
        }
    }

    if had_error {
        std::process::exit(1);
    } else {
        println!("{}", t!("out.done"));
        std::process::exit(0);
    }
}
