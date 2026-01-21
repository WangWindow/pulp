//! TaskId 生成器：提供轻量、自增的任务 ID。

use crate::domain::TaskId;
use std::sync::atomic::{AtomicU64, Ordering};

static TASK_SEQ: AtomicU64 = AtomicU64::new(1);

/// 生成下一个 TaskId（进程内单调递增）。
pub fn next_task_id() -> TaskId {
    TASK_SEQ.fetch_add(1, Ordering::Relaxed)
}
