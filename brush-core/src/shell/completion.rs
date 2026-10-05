//! Command completion support for shell instances.

use crate::{completion, error, extensions};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Generates command completions for the shell.
    ///
    /// # Arguments
    ///
    /// * `input` - The input string to generate completions for.
    /// * `position` - The position in the input string to generate completions at.
    pub async fn complete(
        &mut self,
        input: &str,
        position: usize,
    ) -> Result<completion::Completions, error::Error> {
        let completion_config = self.completion.config.clone();
        completion_config
            .get_completions(self, input, position)
            .await
    }

    /// Returns the options of the programmable completion in progress, if any (e.g.
    /// while its completion function runs), which the `compopt` builtin changes.
    pub fn in_progress_completion_options_mut(
        &mut self,
    ) -> Option<&mut completion::GenerationOptions> {
        self.completion
            .in_progress
            .as_mut()
            .map(|in_progress| &mut in_progress.options)
    }

    /// Returns a mutable reference to the programmable completion in progress, if any.
    pub(crate) const fn in_progress_completion_mut(
        &mut self,
    ) -> &mut Option<completion::InProgressCompletion> {
        &mut self.completion.in_progress
    }
}
