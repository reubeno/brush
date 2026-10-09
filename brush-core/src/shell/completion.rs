//! Command completion support for shell instances.

use crate::{completion, error, extensions};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Completes the word at the cursor in a line being edited, with programmable
    /// completion (see the [`completion`] module).
    ///
    /// # Arguments
    ///
    /// * `input` - The line being completed.
    /// * `cursor` - The cursor's byte offset in the line.
    /// * `prefs` - The line editor's preferences for how completions edit the line.
    ///
    /// # Errors
    ///
    /// Returns an error if `cursor` isn't a char boundary in `input`.
    pub async fn complete(
        &mut self,
        input: &str,
        cursor: usize,
        prefs: &completion::EditPrefs,
    ) -> Result<completion::Completions, error::Error> {
        completion::complete(self, input, cursor, prefs).await
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

    /// Returns the programmable completion in progress, if any.
    pub(crate) const fn in_progress_completion(&self) -> Option<&completion::InProgressCompletion> {
        self.completion.in_progress.as_ref()
    }

    /// Returns a mutable reference to the programmable completion in progress, if any.
    pub(crate) const fn in_progress_completion_mut(
        &mut self,
    ) -> &mut Option<completion::InProgressCompletion> {
        &mut self.completion.in_progress
    }
}
