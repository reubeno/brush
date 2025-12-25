pub(crate) fn format_error(
    err: &brush_core::error::Error,
    _shell: &brush_core::Shell,
    use_color: bool,
) -> String {
    let prefix = if use_color {
        color_print::cstr!("<red>error:</red> ")
    } else {
        "error: "
    };

    std::format!("{prefix}{err:#}\n")
}
