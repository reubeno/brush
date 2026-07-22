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

        // When running with privileges (setuid/setgid where effective UID/GID
        // differs from real), refuse to source any init file whose location is
        // controlled by the (untrusted) environment. That includes BASH_ENV and
        // ENV (env-controlled directly) as well as anything under `$HOME` —
        // because `$HOME` is itself environment-controlled, an unprivileged
        // caller can otherwise direct the privileged shell to source
        // attacker-owned `.bash_profile` / `.bashrc` / etc.
        //
        // System-wide init files (`/etc/profile`, `/etc/bash.bashrc`) and
        // explicit `--rcfile` paths remain honored — those are root-owned or
        // CLI-controlled, not attacker-controlled. This mirrors bash's
        // behavior under `-p`.
        let privileged = crate::sys::users::is_privileged();

        if self.options.login_shell {
            // --noprofile means skip this.
            if matches!(profile_behavior, ProfileLoadBehavior::Skip) {
                return Ok(());
            }

            //
            // Source the system profile if it exists.
            //
            // Next source the first of these that exists and is readable (if any):
            //     * ~/.bash_profile
            //     * ~/.bash_login
            //     * ~/.profile
            //
            if let Some(system_profile) = crate::sys::fs::get_system_profile_path() {
                self.source_if_exists(system_profile, &params).await?;
            }
            if !privileged && let Some(home_path) = self.home_dir() {
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
                        // (CLI-controlled, not env-controlled — honored even under privilege.)
                        self.source_if_exists(rc_file, &params).await?;
                    }
                    RcLoadBehavior::LoadDefault => {
                        //
                        // Otherwise, for non-login interactive shells, load in this order:
                        //
                        //     system rc file (e.g. /etc/bash.bashrc on Unix)
                        //     ~/.bashrc
                        //
                        if let Some(system_rc) = crate::sys::fs::get_system_rc_path() {
                            self.source_if_exists(system_rc, &params).await?;
                        }
                        if !privileged && let Some(home_path) = self.home_dir() {
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

                // Per bash(1) INVOCATION: when BASH_ENV is set on a non-interactive
                // non-login shell (or ENV in POSIX/sh mode), the value is subjected to
                // parameter expansion, command substitution, and arithmetic expansion,
                // and the resulting filename is sourced if it exists. If the file does
                // not exist or is not readable, bash silently continues. (The
                // privilege-skip above already short-circuits this in setuid contexts.)
                if !privileged
                    && let Some(raw_value) = self.env_str(env_var_name).map(|v| v.into_owned())
                {
                    let expanded =
                        crate::expansion::basic_expand_word(self, &params, raw_value).await?;
                    if !expanded.is_empty() {
                        self.source_if_exists(Path::new(&expanded), &params).await?;
                    }
                }
            }
        }

        Ok(())
    }
}
