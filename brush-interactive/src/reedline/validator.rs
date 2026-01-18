use crate::refs;

pub(crate) struct ReedlineValidator<SE: brush_core::ShellExtensions> {
    pub shell: refs::ShellRef<SE>,
}

impl<SE: brush_core::ShellExtensions> reedline::Validator for ReedlineValidator<SE> {
    fn validate(&self, line: &str) -> reedline::ValidationResult {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.shell.lock())
        });

        match shell.parse_string(line.to_owned()) {
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
