//! 任务句柄：用于 UI/CLI 保存与取消任务。

use crate::domain::{TaskId, TaskKind};
use crate::portal::cancel::CancellationToken;

/// CancelHandle：用于 UI 保存可取消句柄。
#[derive(Debug, Clone)]
pub struct CancelHandle {
    token: CancellationToken,
}

impl CancelHandle {
    pub fn new(token: CancellationToken) -> Self {
        Self { token }
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
}

/// JobHandle：用于 UI/CLI 管理任务列表。
#[derive(Debug, Clone)]
pub struct JobHandle {
    pub task_id: TaskId,
    pub kind: TaskKind,
    pub title: String,
    pub cancel: CancelHandle,
}
