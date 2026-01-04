#[derive(Clone)]
pub(crate) struct BrushShellBehavior {
    pub use_color: bool,
}

impl Default for BrushShellBehavior {
    fn default() -> Self {
        Self { use_color: true }
    }
}

impl brush_core::ShellBehavior for BrushShellBehavior {
    fn format_error(
        &self,
        err: &brush_core::error::Error,
        _shell: &impl brush_core::ShellRuntime,
    ) -> String {
        let prefix = if self.use_color {
            color_print::cstr!("<red>error:</red> ")
        } else {
            "error: "
        };

        std::format!("{prefix}{err:#}\n")
    }
}
