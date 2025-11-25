use super::refs;

pub(crate) struct ReedlineValidator {
    pub shell: refs::ShellRef,
}

impl reedline::Validator for ReedlineValidator {
    fn validate(&self, line: &str) -> reedline::ValidationResult {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.shell.lock())
        });

        match shell.parse_string(line, &brush_core::SourceInfo::default()) {
            Err(brush_parser::ParseError::Tokenizing { inner, position: _ })
                if inner.is_incomplete() =>
            {
                reedline::ValidationResult::Incomplete
            }
            Err(brush_parser::ParseError::ParsingAtEndOfInput) => {
                reedline::ValidationResult::Incomplete
            }
            _ => reedline::ValidationResult::Complete,
        }
    }
}
