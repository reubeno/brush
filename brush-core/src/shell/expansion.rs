//! Expansion support for shell instances.

use std::borrow::Cow;

use crate::{
    error, expansion, extensions,
    interp::{self, ExecutionParameters},
};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns the current value of the IFS variable, or the default value if it is not set.
    pub fn ifs(&self) -> Cow<'_, str> {
        self.env_var("IFS")
            .filter(|var| var.value().is_set())
            .map_or_else(|| " \t\n".into(), |var| var.value().to_cow_str(self))
    }

    /// Returns the separator that joins the fields of `$*` and `${arr[*]}`: the first
    /// character of IFS, or nothing at all when IFS is empty (an unset IFS is a space).
    pub(crate) fn ifs_joiner(&self) -> String {
        self.ifs()
            .chars()
            .next()
            .map_or_else(String::new, String::from)
    }

    /// Applies basic shell expansion to the provided string.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand.
    pub async fn basic_expand_string<S: AsRef<str>>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<String, error::Error> {
        let result = expansion::basic_expand_word(self, params, s.as_ref()).await?;
        Ok(result)
    }

    /// Applies full shell expansion and field splitting to the provided string; returns
    /// a sequence of fields.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand and split.
    pub async fn full_expand_and_split_string<S: AsRef<str>>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<Vec<String>, error::Error> {
        let result = expansion::full_expand_and_split_word(self, params, s.as_ref()).await?;
        Ok(result)
    }

    /// Expands an assignment's value as the shell does for an assignment it runs: a
    /// scalar, and each keyed element of an array and its key, as one word; each element
    /// without a key as words that are split and expanded in full.
    ///
    /// # Arguments
    ///
    /// * `params` - The execution parameters to use during expansion.
    /// * `value` - The value to expand.
    pub async fn expand_assignment_value(
        &mut self,
        params: &ExecutionParameters,
        value: &brush_parser::ast::AssignmentValue,
    ) -> Result<brush_parser::ast::AssignmentValue, error::Error> {
        interp::expand_assignment_value(self, params, value).await
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use brush_parser::ast::{AssignmentValue, Word};

    use crate::error;

    #[tokio::test]
    async fn an_array_value_expands_as_an_assignment_does() -> Result<(), error::Error> {
        let mut shell = crate::Shell::builder()
            .profile(crate::ProfileLoadBehavior::Skip)
            .rc(crate::RcLoadBehavior::Skip)
            .build()
            .await?;
        let params = shell.default_exec_params();
        shell.set_env_global("words", crate::ShellVariable::new("a b"))?;
        shell.set_env_global("key", crate::ShellVariable::new("k"))?;

        let value = AssignmentValue::Array(vec![
            (None, Word::from("${words}".to_owned())),
            (
                Some(Word::from("$key".to_owned())),
                Word::from("$words".to_owned()),
            ),
        ]);
        let expanded = shell.expand_assignment_value(&params, &value).await?;

        // Elements without a key are split into words; a keyed one, and its key, are not.
        let elements: Vec<_> = match &expanded {
            AssignmentValue::Array(elements) => elements
                .iter()
                .map(|(key, value)| (key.as_ref().map(|k| k.value.as_str()), value.value.as_str()))
                .collect(),
            AssignmentValue::Scalar(_) => vec![],
        };
        assert_eq!(elements, [(None, "a"), (None, "b"), (Some("k"), "a b")]);
        Ok(())
    }
}
