//! Expansion support for shell instances.

use std::borrow::Cow;

use brush_parser::ast;

use crate::{error, expansion, extensions, interp::ExecutionParameters, variables::ArrayKind};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns the current value of the IFS variable, or the default value if it is not set.
    pub fn ifs(&self) -> Cow<'_, str> {
        self.env_str("IFS").unwrap_or_else(|| " \t\n".into())
    }

    /// Returns the first character of the IFS variable, or a space if it is not set.
    pub(crate) fn get_ifs_first_char(&self) -> char {
        self.ifs().chars().next().unwrap_or(' ')
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

    /// Expands a raw parsed assignment and resolves its subscripts against `target`. See
    /// `expansion::expand_assignment`.
    ///
    /// # Arguments
    ///
    /// * `params` - The execution parameters to use during expansion.
    /// * `assignment` - The parsed assignment to expand.
    /// * `target` - The target array type controlling subscript expansion.
    pub async fn expand_assignment(
        &mut self,
        params: &ExecutionParameters,
        assignment: &ast::Assignment,
        target: ArrayKind,
    ) -> Result<expansion::ResolvedAssignment, error::Error> {
        expansion::expand_assignment(self, params, assignment, target).await
    }

    /// Resolves one array subscript against the kind of the array it names. See
    /// `expansion::resolve_array_subscript`.
    ///
    /// # Arguments
    ///
    /// * `params` - The execution parameters to use during expansion.
    /// * `index` - The subscript, as written after the operand's own word expansion.
    /// * `kind` - The target array type controlling subscript expansion.
    pub async fn resolve_array_subscript(
        &mut self,
        params: &ExecutionParameters,
        index: &str,
        kind: ArrayKind,
    ) -> Result<String, error::Error> {
        expansion::resolve_array_subscript(self, params, index, kind).await
    }

    /// Resolves the subscripts of an assignment whose words were already expanded, leaving its
    /// values untouched. See `expansion::resolve_assignment_subscripts`.
    ///
    /// # Arguments
    ///
    /// * `params` - The execution parameters to use during expansion.
    /// * `assignment` - The already-word-expanded assignment to resolve.
    /// * `target` - The target array type controlling subscript expansion.
    pub async fn resolve_assignment_subscripts(
        &mut self,
        params: &ExecutionParameters,
        assignment: ast::Assignment,
        target: ArrayKind,
    ) -> Result<expansion::ResolvedAssignment, error::Error> {
        expansion::resolve_assignment_subscripts(self, params, assignment, target).await
    }
}
