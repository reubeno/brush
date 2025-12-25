use crate::input_backend::InteractivePrompt;

impl reedline::Prompt for InteractivePrompt {
    fn render_prompt_left(&self) -> std::borrow::Cow<'_, str> {
        // [Workaround: see https://github.com/nushell/reedline/issues/707]
        // If the prompt starts with a newline character, then there's a chance
        // that it won't be rendered correctly. For this specific case, insert
        // an extra space character before the newline.
        if self.prompt.starts_with('\n') {
            std::format!(" {}", self.prompt).into()
        } else {
            self.prompt.as_str().into()
        }
    }

    fn render_prompt_right(&self) -> std::borrow::Cow<'_, str> {
        self.alt_side_prompt.as_str().into()
    }

    // N.B. For now, we don't support prompt indicators.
    fn render_prompt_indicator(
        &self,
        _prompt_mode: reedline::PromptEditMode,
    ) -> std::borrow::Cow<'_, str> {
        "".into()
    }

    fn render_prompt_multiline_indicator(&self) -> std::borrow::Cow<'_, str> {
        self.continuation_prompt.as_str().into()
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: reedline::PromptHistorySearch,
    ) -> std::borrow::Cow<'_, str> {
        match history_search.status {
            reedline::PromptHistorySearchStatus::Passing => {
                if history_search.term.is_empty() {
                    "(rev search) ".into()
                } else {
                    std::format!("(rev search: {}) ", history_search.term).into()
                }
            }
            reedline::PromptHistorySearchStatus::Failing => {
                std::format!("(failing rev search: {}) ", history_search.term).into()
            }
        }
    }

    fn get_prompt_color(&self) -> reedline::Color {
        reedline::Color::Reset
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
