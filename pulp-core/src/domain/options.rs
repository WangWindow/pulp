/// 列出条目选项（list）。
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// 密码（如需）。
    pub password: Option<String>,
}

/// 解压选项（extract）。
#[derive(Debug, Clone)]
pub struct ExtractOptions {
    /// 是否覆盖已存在文件。
    pub overwrite: bool,

    /// 是否保留压缩包内路径结构。
    pub preserve_paths: bool,

    /// 剥离前 N 级路径（类似 tar --strip-components）。
    pub strip_components: Option<usize>,

    /// 密码（如需）。
    pub password: Option<String>,

    /// 线程数提示（backend 可选择性使用）。
    pub threads: Option<usize>,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            overwrite: false,
            preserve_paths: true,
            strip_components: None,
            password: None,
            threads: None,
        }
    }
}

/// 压缩选项（compress）。
#[derive(Debug, Clone)]
pub struct CompressOptions {
    /// 压缩等级（语义由后端决定，通常 0-9）。
    pub level: Option<u8>,

    /// 是否启用 solid（仅对 7z 有意义）。
    pub solid: Option<bool>,

    /// 密码（如需）。
    pub password: Option<String>,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            level: None,
            solid: None,
            password: None,
        }
    }
}
