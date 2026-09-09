//! Function support for shells.

use crate::{
    ExecutionParameters, ExecutionResult, commands, error, extensions, functions,
    results::ExecutionWaitResult,
};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns the function definition environment for this shell.
    pub const fn funcs(&self) -> &functions::FunctionEnv {
        &self.funcs
    }

    /// Returns a mutable reference to the function definition environment for this shell.
    pub const fn funcs_mut(&mut self) -> &mut functions::FunctionEnv {
        &mut self.funcs
    }

    /// Tries to undefine a function in the shell's environment. Returns whether or
    /// not a definition was removed. A readonly function is refused.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to undefine.
    pub fn undefine_func(&mut self, name: &str) -> Result<bool, error::Error> {
        if self.funcs.get(name).is_some_and(|reg| reg.is_readonly()) {
            return Err(error::ErrorKind::ReadonlyFunction(name.to_owned()).into());
        }

        Ok(self.funcs.remove(name).is_some())
    }

    /// Defines a function in the shell's environment. If a function already exists
    /// with the given name, it is replaced with the new definition -- unless it is
    /// readonly, in which case the definition is refused.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to define.
    /// * `definition` - The function's definition.
    /// * `source_info` - Source information for the function definition.
    pub fn define_func(
        &mut self,
        name: impl Into<String>,
        definition: brush_parser::ast::FunctionDefinition,
        source_info: &crate::SourceInfo,
    ) -> Result<(), error::Error> {
        let name = name.into();
        if self.funcs.get(&name).is_some_and(|reg| reg.is_readonly()) {
            return Err(error::ErrorKind::ReadonlyFunction(name).into());
        }

        let reg = functions::Registration::new(definition, source_info);
        self.funcs.update(name, reg);
        Ok(())
    }

    /// Tries to return a mutable reference to the registration for a named function.
    /// Returns `None` if no such function was found.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to lookup
    pub fn func_mut(&mut self, name: &str) -> Option<&mut functions::Registration> {
        self.funcs.get_mut(name)
    }

    /// Tries to define a function in the shell's environment using the given
    /// string as its body.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function
    /// * `body_text` - The body of the function, expected to start with "()".
    pub fn define_func_from_str(
        &mut self,
        name: impl Into<String>,
        body_text: &str,
    ) -> Result<(), error::Error> {
        let name = name.into();

        let mut parser =
            super::parsing::create_parser(body_text.as_bytes(), &self.parser_options());
        let func_body = parser.parse_function_parens_and_body().map_err(|e| {
            error::Error::from(error::ErrorKind::FunctionParseError(name.clone(), e))
        })?;

        let def = brush_parser::ast::FunctionDefinition {
            fname: name.clone().into(),
            body: func_body,
        };

        self.define_func(name, def, &crate::SourceInfo::default())
    }

    /// Invokes a function defined in this shell, returning its execution result.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to invoke.
    /// * `args` - The arguments to pass to the function.
    /// * `params` - Execution parameters to use for the invocation.
    pub async fn invoke_function<N: AsRef<str>, I: IntoIterator<Item = A>, A: AsRef<str>>(
        &mut self,
        name: N,
        args: I,
        params: ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let name = name.as_ref();
        let command_name = String::from(name);

        let func_registration = self
            .funcs
            .get(name)
            .ok_or_else(|| error::ErrorKind::FunctionNotFound(name.to_owned()))?
            .to_owned();

        let context = commands::ExecutionContext {
            shell: self,
            command_name,
            params,
        };

        let command_args = args
            .into_iter()
            .map(|s| commands::CommandArg::String(String::from(s.as_ref())))
            .collect::<Vec<_>>();

        let result =
            commands::invoke_shell_function(func_registration, context, &command_args).await?;

        match result.wait().await? {
            ExecutionWaitResult::Completed(result) => Ok(result),
            ExecutionWaitResult::Stopped(..) => {
                error::unimp("stopped child from function invocation")
            }
        }
    }
}
