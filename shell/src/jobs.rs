use std::collections::VecDeque;
use std::fmt::Display;

use futures::FutureExt;

use crate::error;
use crate::ExecutionResult;

pub(crate) type JobJoinHandle = tokio::task::JoinHandle<Result<ExecutionResult, error::Error>>;
pub(crate) type JobResult = (Job, Result<ExecutionResult, error::Error>);

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

    #[allow(clippy::unused_self)]
    pub fn resolve_job_spec(&self, _job_spec: &str) -> Option<&mut Job> {
        tracing::warn!("resolve_job_spec is not implemented");
        None
    }

    pub async fn wait_all(&mut self) -> Result<Vec<Job>, error::Error> {
        for job in &mut self.background_jobs {
            job.wait().await?;
        }

        Ok(self.sweep_completed_jobs())
    }

    pub fn poll(&mut self) -> Result<Vec<JobResult>, error::Error> {
        let mut results = vec![];

        let mut i = 0;
        while i != self.background_jobs.len() {
            if let Some(result) = self.background_jobs[i].poll_done()? {
                let job = self.background_jobs.remove(i);
                results.push((job, result));
            } else {
                i += 1;
            }
        }

        Ok(results)
    }

    fn sweep_completed_jobs(&mut self) -> Vec<Job> {
        let mut completed_jobs = vec![];

        let mut i = 0;
        while i != self.background_jobs.len() {
            if self.background_jobs[i].join_handles.is_empty() {
                completed_jobs.push(self.background_jobs.remove(i));
            } else {
                i += 1;
            }
        }

        completed_jobs
    }
}

#[derive(Clone)]
pub enum JobState {
    Unknown,
    Running,
    Stopped,
    Done,
}

impl Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobState::Unknown => write!(f, "Unknown"),
            JobState::Running => write!(f, "Running"),
            JobState::Stopped => write!(f, "Stopped"),
            JobState::Done => write!(f, "Done"),
        }
    }
}

#[derive(Clone)]
pub enum JobAnnotation {
    None,
    Current,
    Previous,
}

impl Display for JobAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobAnnotation::None => write!(f, ""),
            JobAnnotation::Current => write!(f, "+"),
            JobAnnotation::Previous => write!(f, "-"),
        }
    }
}

pub struct Job {
    join_handles: VecDeque<JobJoinHandle>,
    pids: Vec<u32>,
    annotation: JobAnnotation,

    pub id: usize,
    pub command_line: String,
    pub state: JobState,
}

impl Display for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}]{:3}{}\t{}",
            self.id,
            self.annotation.to_string(),
            self.state,
            self.command_line
        )
    }
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

    pub fn get_annotation(&self) -> JobAnnotation {
        self.annotation.clone()
    }

    pub fn is_current(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Current)
    }

    pub fn is_prev(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Previous)
    }

    pub fn poll_done(
        &mut self,
    ) -> Result<Option<Result<ExecutionResult, error::Error>>, error::Error> {
        let mut result: Option<Result<ExecutionResult, error::Error>> = None;

        while !self.join_handles.is_empty() {
            let join_handle = &mut self.join_handles[0];
            match join_handle.now_or_never() {
                Some(Ok(r)) => {
                    self.join_handles.remove(0);
                    result = Some(r);
                }
                Some(Err(e)) => {
                    self.join_handles.remove(0);
                    return Err(error::Error::ThreadingError(e));
                }
                None => return Ok(None),
            }
        }

        self.state = JobState::Done;

        Ok(result)
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

    #[allow(clippy::unnecessary_wraps)]
    pub fn get_pid(&self) -> Result<Option<u32>, error::Error> {
        if self.pids.is_empty() {
            tracing::debug!("UNIMPLEMENTED: get pid for job");
            Ok(None)
        } else {
            Ok(Some(self.pids[0]))
        }
    }
}
