use std::collections::VecDeque;
use std::fmt::Display;

use crate::error;
use crate::ExecutionResult;

pub(crate) type JobJoinHandle = tokio::task::JoinHandle<Result<ExecutionResult, error::Error>>;

#[derive(Default)]
pub struct JobManager {
    pub background_jobs: Vec<Job>,
}

impl JobManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, mut job: Job) -> &Job {
        let id = self.background_jobs.len() + 1;
        job.id = id;
        self.background_jobs.push(job);
        self.background_jobs.last().unwrap()
    }

    pub fn current_job(&self) -> Option<&Job> {
        // TODO: Properly track current.
        self.background_jobs.last()
    }

    pub fn current_job_mut(&mut self) -> Option<&mut Job> {
        // TODO: Properly track current.
        self.background_jobs.last_mut()
    }

    pub fn resolve_job_spec(&self, _job_spec: &str) -> Option<&mut Job> {
        todo!("resolve_job_spec")
    }
}

#[derive(Clone)]
pub enum JobState {
    Unknown,
    Running,
    Stopped,
}

impl Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobState::Unknown => write!(f, "Unknown"),
            JobState::Running => write!(f, "Running"),
            JobState::Stopped => write!(f, "Stopped"),
        }
    }
}

#[allow(dead_code)]
enum JobAnnotation {
    None,
    Current,
    Previous,
}

pub struct Job {
    join_handles: VecDeque<JobJoinHandle>,
    #[allow(dead_code)]
    pids: Vec<u32>,
    annotation: JobAnnotation,

    pub id: usize,
    pub command_line: String,
    pub state: JobState,
}

impl Job {
    pub(crate) fn new(
        join_handles: VecDeque<JobJoinHandle>,
        pids: Vec<u32>,
        command_line: String,
        state: JobState,
    ) -> Self {
        Self {
            id: 0,
            join_handles,
            pids,
            annotation: JobAnnotation::None,
            command_line,
            state,
        }
    }

    pub fn is_current(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Current)
    }

    pub fn is_prev(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Previous)
    }

    pub async fn wait(&mut self) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        while let Some(join_handle) = self.join_handles.pop_back() {
            result = join_handle.await.unwrap()?;
        }

        Ok(result)
    }

    #[allow(clippy::unused_self)]
    pub fn move_to_background(&mut self) -> Result<(), error::Error> {
        error::unimp("move job to background")
    }

    #[cfg(unix)]
    pub fn move_to_foreground(&mut self) -> Result<(), error::Error> {
        if !matches!(self.state, JobState::Stopped) {
            return error::unimp("move job to foreground for not stopped job");
        }

        #[allow(clippy::cast_possible_wrap)]
        if let Some(pid) = self.get_pid()? {
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::SIGCONT,
            )
            .map_err(|_errno| error::Error::FailedToSendSignal)?;

            self.state = JobState::Running;
            Ok(())
        } else {
            Err(error::Error::FailedToSendSignal)
        }
    }

    #[cfg(not(unix))]
    pub fn move_to_foreground(&mut self) -> Result<(), error::Error> {
        error::unimp("move job to foreground")
    }

    #[cfg(unix)]
    pub fn kill(&mut self) -> Result<(), error::Error> {
        if let Some(pid) = self.get_pid()? {
            #[allow(clippy::cast_possible_wrap)]
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::SIGKILL,
            )
            .map_err(|_errno| error::Error::FailedToSendSignal)?;
            Ok(())
        } else {
            Err(error::Error::FailedToSendSignal)
        }
    }

    #[cfg(not(unix))]
    pub fn kill(&mut self) -> Result<(), error::Error> {
        error::unimp("kill job")
    }

    #[cfg(unix)]
    fn get_pid(&self) -> Result<Option<u32>, error::Error> {
        if self.pids.is_empty() {
            error::unimp("get pid for job")
        } else if self.pids.len() > 1 {
            error::unimp("get pid for job with multiple pids")
        } else {
            Ok(Some(self.pids[0]))
        }
    }
}
