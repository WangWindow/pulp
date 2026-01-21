//! 默认实现入口（defaults）
//!
//! 该模块提供“推荐的默认构造器”，让 UI/CLI 以最少的样板代码拿到可用的 `ArchiveService`。

use crate::{backend::create_default_registry, portal::service::DefaultArchiveService};

/// 创建默认的 `ArchiveService` 实例。
///
/// 默认策略：
/// - 使用 `backend::create_default_registry()` 注册内置后端集合；
/// - 返回 `DefaultArchiveService`。
///
/// 典型用法（UI/CLI）：
/// - `let service = pulp_core::create_default_service();`
pub fn create_default_service() -> DefaultArchiveService {
    DefaultArchiveService::new(create_default_registry())
}
