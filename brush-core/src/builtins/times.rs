use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, error};

/// Report on usage time.
#[derive(Parser)]
pub(crate) struct TimesCommand {}

impl builtins::Command for TimesCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, error::Error> {
        let self_usage = nix::sys::resource::getrusage(nix::sys::resource::UsageWho::RUSAGE_SELF)?;

        writeln!(
            context.stdout(),
            "{} {}",
            format_time(self_usage.user_time()),
            format_time(self_usage.system_time()),
        )?;

        let children_usage =
            nix::sys::resource::getrusage(nix::sys::resource::UsageWho::RUSAGE_CHILDREN)?;

        writeln!(
            context.stdout(),
            "{} {}",
            format_time(children_usage.user_time()),
            format_time(children_usage.system_time()),
        )?;

        Ok(builtins::ExitCode::Success)
    }
}

fn format_time(time: nix::sys::time::TimeVal) -> String {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let duration = std::time::Duration::new(time.tv_sec() as u64, time.tv_usec() as u32 * 1000);
    let minutes = duration.as_secs() / 60;
    let seconds = duration.as_secs() % 60;
    let millis = duration.subsec_millis();
    format!("{minutes}m{seconds}.{millis:03}s")
}

#[cfg(test)]
mod tests {
    use nix::sys::time::TimeValLike;

    use super::*;

    #[test]
    fn test_format_time() {
        fn ms_to_timeval(ms: i64) -> nix::sys::time::TimeVal {
            nix::sys::time::TimeVal::milliseconds(ms)
        }

        fn us_to_timeval(us: i64) -> nix::sys::time::TimeVal {
            nix::sys::time::TimeVal::microseconds(us)
        }

        assert_eq!(format_time(ms_to_timeval(0)), "0m0.000s");
        assert_eq!(format_time(ms_to_timeval(1)), "0m0.001s");
        assert_eq!(format_time(ms_to_timeval(123)), "0m0.123s");
        assert_eq!(format_time(ms_to_timeval(1234)), "0m1.234s");
        assert_eq!(format_time(ms_to_timeval(12345)), "0m12.345s");
        assert_eq!(format_time(ms_to_timeval(123456)), "2m3.456s");
        assert_eq!(format_time(ms_to_timeval(1234567)), "20m34.567s");

        assert_eq!(format_time(us_to_timeval(1)), "0m0.000s");
        assert_eq!(format_time(us_to_timeval(999)), "0m0.000s");
        assert_eq!(format_time(us_to_timeval(1000)), "0m0.001s");
    }
}
