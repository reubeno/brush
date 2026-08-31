//! Expansion support for shell instances.

use std::borrow::Cow;

use brush_parser::ast;

use crate::{env, error, expansion, extensions, interp::ExecutionParameters, variables::ArrayKind};

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

    /// Expands a raw parsed assignment according to shell assignment rules. Returns the assignment
    /// with its optional subscript and value expanded; its grammar-validated base variable name is
    /// unchanged.
    ///
    /// The assignment's words must not already have undergone argument expansion, or they will be
    /// expanded twice. A declaration builtin may still use this for compound elements it has just
    /// recognized inside an operand: the operand was expanded once, but that expansion could not
    /// see the words hidden inside its compound syntax.
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
    ) -> Result<ast::Assignment, error::Error> {
        expansion::expand_assignment(self, params, assignment, target).await
    }

    /// Returns the array kind of the variable that `name` resolves to under `lookup`, or `None`
    /// when no such variable exists or it is not an array.
    ///
    /// Declaration builtins use this to answer two questions from one lookup: whether the target
    /// is already an array, and — absent an explicit `-a`/`-A` — which subscript rules its
    /// assignments follow.
    ///
    /// # Arguments
    ///
    /// * `name` - The variable name to inspect, without any subscript.
    /// * `lookup` - The scope policy governing which variable, if any, the name resolves to.
    pub fn existing_array_kind(
        &self,
        name: &str,
        lookup: env::EnvironmentLookup,
    ) -> Option<ArrayKind> {
        self.env()
            .get_using_policy(name, lookup)?
            .value()
            .array_kind()
    }

    /// Resolves the subscripts of an assignment whose words a shell already expanded, and returns
    /// the updated assignment. Values are left exactly as they are, so an assignment is never
    /// expanded twice. Declaration builtins use this on the [`crate::CommandArg::Assignment`]
    /// operands they receive, once their options reveal the target array type.
    ///
    /// # Arguments
    ///
    /// * `params` - The execution parameters to use during expansion.
    /// * `assignment` - The already-word-expanded assignment to resolve.
    /// * `target` - The target array type controlling subscript expansion.
    pub async fn resolve_assignment_subscripts(
        &mut self,
        params: &ExecutionParameters,
        assignment: &ast::Assignment,
        target: ArrayKind,
    ) -> Result<ast::Assignment, error::Error> {
        expansion::resolve_assignment_subscripts(self, params, assignment, target).await
    }
}
