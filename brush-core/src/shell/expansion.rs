//! Expansion support for shell instances.

use std::borrow::Cow;

use crate::{error, expansion, extensions, interp::ExecutionParameters};

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
}
