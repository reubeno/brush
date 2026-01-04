use crate::refs;

pub(crate) struct ReedlineValidator<S: brush_core::ShellRuntime> {
    pub shell: refs::ShellRef<S>,
}

impl<S: brush_core::ShellRuntime> reedline::Validator for ReedlineValidator<S> {
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
