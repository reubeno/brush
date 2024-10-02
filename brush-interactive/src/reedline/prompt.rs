use crate::interactive_shell::InteractivePrompt;

impl reedline::Prompt for InteractivePrompt {
    fn render_prompt_left(&self) -> std::borrow::Cow<str> {
        self.prompt.as_str().into()
    }

    fn render_prompt_right(&self) -> std::borrow::Cow<str> {
        self.alt_side_prompt.as_str().into()
    }

    // N.B. For now, we don't support prompt indicators.
    fn render_prompt_indicator(
        &self,
        _prompt_mode: reedline::PromptEditMode,
    ) -> std::borrow::Cow<str> {
        "".into()
    }

    fn render_prompt_multiline_indicator(&self) -> std::borrow::Cow<str> {
        self.continuation_prompt.as_str().into()
    }

    // TODO: Decide what to display.
    fn render_prompt_history_search_indicator(
        &self,
        _history_search: reedline::PromptHistorySearch,
    ) -> std::borrow::Cow<str> {
        "(hist-search) ".into()
    }

    fn get_prompt_color(&self) -> reedline::Color {
        reedline::Color::Magenta
    }

    fn get_prompt_multiline_color(&self) -> nu_ansi_term::Color {
        nu_ansi_term::Color::LightBlue
    }

    fn get_indicator_color(&self) -> reedline::Color {
        reedline::Color::Cyan
    }

    fn get_prompt_right_color(&self) -> reedline::Color {
        reedline::Color::AnsiValue(5)
    }

    fn right_prompt_on_last_line(&self) -> bool {
        false
    }
}
