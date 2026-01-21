//! 取消（Cancellation）与协作式中断。
//!
//! 设计目标：
//! - **不绑定运行时**：不依赖 tokio。
//! - **协作式取消**：后端在长循环中主动检查。

use crate::portal::error::{PulpError, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// 协作式取消令牌。
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 请求取消（幂等）。
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// 是否已请求取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// 若已取消则返回错误。
    pub fn throw_if_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(PulpError::Cancelled)
        } else {
            Ok(())
        }
    }
}
