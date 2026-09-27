//! Effect-level contracts for commands short-circuited by static filters.
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]
#![allow(
    clippy::unused_async_trait_impl,
    reason = "Test hooks defer their effects until polled"
)]

use brush_core::extensions::{DefaultErrorFormatter, ShellExtensionsImpl};
use brush_core::filter::{
    CmdExecFilter, ExternalCmdOutput, ExternalCmdParams, NoOpSourceFilter, PreFilterResult,
    SimpleCmdOutput, SimpleCmdParams,
};

#[derive(Clone, Default)]
struct Denier {
    external: bool,
}

type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, Denier, NoOpSourceFilter>;

fn denied() -> brush_core::ExecutionSpawnResult {
    brush_core::ExecutionSpawnResult::Completed(brush_core::ExecutionExitCode::from(1u8).into())
}

impl CmdExecFilter for Denier {
    async fn pre_simple_cmd<'a, SE: brush_core::ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        if self.external {
            PreFilterResult::Continue(params)
        } else {
            PreFilterResult::Return(Ok(denied()))
        }
    }

    async fn pre_external_cmd<'a, SE: brush_core::ShellExtensions>(
        &self,
        _params: ExternalCmdParams<'a, SE>,
    ) -> PreFilterResult<ExternalCmdParams<'a, SE>, ExternalCmdOutput> {
        PreFilterResult::Return(Ok(denied()))
    }
}

async fn assert_temporary_environment_is_removed(
    external: bool,
    command: &str,
) -> Result<(), brush_core::Error> {
    let mut shell = brush_core::Shell::builder_with_extensions::<Extensions>()
        .cmd_exec_filter(Denier { external })
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .build()
        .await?;
    let params = shell.default_exec_params();
    shell
        .run_string(command, &brush_core::SourceInfo::default(), &params)
        .await?;
    assert_eq!(
        shell.env_str("FILTER_TEMP"),
        None,
        "a filter short-circuit must pop the command's temporary environment"
    );
    Ok(())
}

#[tokio::test]
async fn simple_filter_short_circuit_restores_environment() -> Result<(), brush_core::Error> {
    assert_temporary_environment_is_removed(false, "FILTER_TEMP=one blocked").await
}

#[cfg(unix)]
#[tokio::test]
async fn external_filter_short_circuit_restores_environment() -> Result<(), brush_core::Error> {
    assert_temporary_environment_is_removed(true, "FILTER_TEMP=one /usr/bin/true").await
}

#[cfg(unix)]
mod final_authorization {
    use super::*;
    use brush_core::filter::FilterStack;
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    #[derive(Clone, Default)]
    struct Authorizer(Arc<AtomicBool>);

    impl CmdExecFilter for Authorizer {
        async fn authorize_external_cmd<SE: brush_core::ShellExtensions>(
            &self,
            params: &ExternalCmdParams<'_, SE>,
        ) -> Result<(), brush_core::Error> {
            self.0.store(true, Ordering::SeqCst);
            assert_eq!(params.original_command(), "/usr/bin/true");
            assert_eq!(params.command.argv0(), "/usr/bin/true");
            if params.command.program() == "/usr/bin/true" {
                Ok(())
            } else {
                Err(
                    brush_core::Error::from(brush_core::ErrorKind::PermissionDenied)
                        .into_terminating(),
                )
            }
        }
    }

    #[derive(Clone, Default)]
    struct Rewriter(PathBuf);

    impl CmdExecFilter for Rewriter {
        async fn pre_external_cmd<'a, SE: brush_core::ShellExtensions>(
            &self,
            mut params: ExternalCmdParams<'a, SE>,
        ) -> PreFilterResult<ExternalCmdParams<'a, SE>, ExternalCmdOutput> {
            params.command.set_program("/usr/bin/touch").arg(&self.0);
            PreFilterResult::Continue(params)
        }
    }

    #[tokio::test]
    async fn later_rewrite_is_authorized_before_any_process_effect() -> anyhow::Result<()> {
        type Extensions =
            ShellExtensionsImpl<DefaultErrorFormatter, FilterStack<Authorizer, Rewriter>>;
        let dir = tempfile::tempdir()?;
        let marker = dir.path().join("must-not-exist");
        let authorizer = Authorizer::default();
        let mut shell = brush_core::Shell::builder_with_extensions::<Extensions>()
            .cmd_exec_filter(FilterStack::new(
                authorizer.clone(),
                Rewriter(marker.clone()),
            ))
            .build()
            .await?;
        let params = shell.default_exec_params();
        let result = shell
            .run_string(
                "FILTER_TEMP=one /usr/bin/true",
                &brush_core::SourceInfo::default(),
                &params,
            )
            .await;
        assert!(
            !marker.exists(),
            "a rewrite must not bypass an earlier policy"
        );
        assert!(result.is_err());
        assert!(authorizer.0.load(Ordering::SeqCst));
        assert_eq!(shell.env_str("FILTER_TEMP"), None);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[derive(Clone, Default)]
    struct NativePathRewrite {
        program: PathBuf,
        marker: PathBuf,
    }

    #[cfg(target_os = "linux")]
    impl CmdExecFilter for NativePathRewrite {
        async fn pre_external_cmd<'a, SE: brush_core::ShellExtensions>(
            &self,
            mut params: ExternalCmdParams<'a, SE>,
        ) -> PreFilterResult<ExternalCmdParams<'a, SE>, ExternalCmdOutput> {
            params.command.set_program(&self.program).arg(&self.marker);
            PreFilterResult::Continue(params)
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn rewritten_native_program_and_argument_reach_os_without_loss() -> anyhow::Result<()> {
        use std::os::unix::{ffi::OsStringExt as _, fs::symlink};
        type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, NativePathRewrite>;
        let dir = tempfile::tempdir()?;
        let program = dir
            .path()
            .join(std::ffi::OsString::from_vec(b"program-\xff".to_vec()));
        let marker = dir
            .path()
            .join(std::ffi::OsString::from_vec(b"marker-\xfe".to_vec()));
        symlink("/usr/bin/touch", &program)?;
        let mut shell = brush_core::Shell::builder_with_extensions::<Extensions>()
            .cmd_exec_filter(NativePathRewrite {
                program,
                marker: marker.clone(),
            })
            .build()
            .await?;
        let params = shell.default_exec_params();
        let result = shell
            .run_string("/usr/bin/true", &brush_core::SourceInfo::default(), &params)
            .await?;
        assert!(result.is_success());
        assert!(
            marker.exists(),
            "the exact native argument must reach the OS"
        );
        Ok(())
    }

    #[derive(Clone, Default)]
    struct NativeArgumentRewrite;

    impl CmdExecFilter for NativeArgumentRewrite {
        async fn pre_external_cmd<'a, SE: brush_core::ShellExtensions>(
            &self,
            mut params: ExternalCmdParams<'a, SE>,
        ) -> PreFilterResult<ExternalCmdParams<'a, SE>, ExternalCmdOutput> {
            use std::os::unix::ffi::OsStringExt as _;
            params.command.set_program("/usr/bin/printf").arg("%s").arg(
                std::ffi::OsString::from_vec(b"native-\xff-argument".to_vec()),
            );
            PreFilterResult::Continue(params)
        }
    }

    #[tokio::test]
    async fn rewritten_native_argument_reaches_os_without_loss() -> anyhow::Result<()> {
        type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, NativeArgumentRewrite>;
        let dir = tempfile::tempdir()?;
        let mut shell = brush_core::Shell::builder_with_extensions::<Extensions>()
            .cmd_exec_filter(NativeArgumentRewrite)
            .working_dir(dir.path().to_owned())
            .build()
            .await?;
        let params = shell.default_exec_params();
        let result = shell
            .run_string(
                "/usr/bin/true > output",
                &brush_core::SourceInfo::default(),
                &params,
            )
            .await?;
        assert!(result.is_success());
        assert_eq!(
            std::fs::read(dir.path().join("output"))?,
            b"native-\xff-argument"
        );
        Ok(())
    }
}
