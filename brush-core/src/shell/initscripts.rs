//! Init script support for shells.

use std::path::{Path, PathBuf};

use crate::{Shell, error, extensions, interp};

/// Behavior for loading profile files.
#[derive(Default)]
pub enum ProfileLoadBehavior {
    /// Load the default profile files.
    #[default]
    LoadDefault,
    /// Skip loading profile files.
    Skip,
}

impl ProfileLoadBehavior {
    /// Returns whether profile loading should be skipped.
    pub const fn skip(&self) -> bool {
        matches!(self, Self::Skip)
    }
}

/// Behavior for loading rc files.
#[derive(Default)]
pub enum RcLoadBehavior {
    /// Load the default rc files.
    #[default]
    LoadDefault,
    /// Load a custom rc file; do not load defaults.
    LoadCustom(PathBuf),
    /// Skip loading rc files.
    Skip,
}

impl RcLoadBehavior {
    /// Returns whether rc loading should be skipped.
    pub const fn skip(&self) -> bool {
        matches!(self, Self::Skip)
    }
}

impl<SE: extensions::ShellExtensions> Shell<SE> {
    /// Loads and executes standard shell configuration files (i.e., rc and profile).
    ///
    /// # Arguments
    ///
    /// * `profile_behavior` - Behavior for loading profile files.
    /// * `rc_behavior` - Behavior for loading rc files.
    pub async fn load_config(
        &mut self,
        profile_behavior: &ProfileLoadBehavior,
        rc_behavior: &RcLoadBehavior,
    ) -> Result<(), error::Error> {
        let mut params = self.default_exec_params();
        params.process_group_policy = interp::ProcessGroupPolicy::SameProcessGroup;

        if self.options.login_shell {
            // --noprofile means skip this.
            if matches!(profile_behavior, ProfileLoadBehavior::Skip) {
                return Ok(());
            }

            //
            // Source /etc/profile if it exists.
            //
            // Next source the first of these that exists and is readable (if any):
            //     * ~/.bash_profile
            //     * ~/.bash_login
            //     * ~/.profile
            //
            self.source_if_exists(Path::new("/etc/profile"), &params)
                .await?;
            if let Some(home_path) = self.home_dir() {
                if self.options.sh_mode {
                    self.source_if_exists(home_path.join(".profile").as_path(), &params)
                        .await?;
                } else {
                    if !self
                        .source_if_exists(home_path.join(".bash_profile").as_path(), &params)
                        .await?
                    {
                        if !self
                            .source_if_exists(home_path.join(".bash_login").as_path(), &params)
                            .await?
                        {
                            self.source_if_exists(home_path.join(".profile").as_path(), &params)
                                .await?;
                        }
                    }
                }
            }
        } else {
            if self.options.interactive {
                match rc_behavior {
                    _ if self.options.sh_mode => (),
                    RcLoadBehavior::Skip => (),
                    RcLoadBehavior::LoadCustom(rc_file) => {
                        // If an explicit rc file is provided, source it.
                        self.source_if_exists(rc_file, &params).await?;
                    }
                    RcLoadBehavior::LoadDefault => {
                        //
                        // Otherwise, for non-login interactive shells, load in this order:
                        //
                        //     /etc/bash.bashrc
                        //     ~/.bashrc
                        //
                        self.source_if_exists(Path::new("/etc/bash.bashrc"), &params)
                            .await?;
                        if let Some(home_path) = self.home_dir() {
                            self.source_if_exists(home_path.join(".bashrc").as_path(), &params)
                                .await?;
                            self.source_if_exists(home_path.join(".brushrc").as_path(), &params)
                                .await?;
                        }
                    }
                }
            } else {
                let env_var_name = if self.options.sh_mode {
                    "ENV"
                } else {
                    "BASH_ENV"
                };

                if self.env.is_set(env_var_name) {
                    //
                    // TODO(well-known-vars): look at $ENV/BASH_ENV; source its expansion if that
                    // file exists
                    //
                    return error::unimp(
                        "load config from $ENV/BASH_ENV for non-interactive, non-login shell",
                    );
                }
            }
        }

        Ok(())
    }
}
