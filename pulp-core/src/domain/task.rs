/// 任务 ID（由上层生成）。
pub type TaskId = u64;

/// 任务类型（用于 UI 分类/过滤）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskKind {
    List,
    Extract,
    Compress,
}

/// 任务阶段（粗粒度）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskPhase {
    Preparing,
    Scanning,
    Processing,
    Finalizing,
}
