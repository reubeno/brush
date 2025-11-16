//! Job management

use std::collections::VecDeque;
use std::fmt::Display;

use futures::FutureExt;

use crate::ExecutionResult;
use crate::error;
use crate::processes;
use crate::sys;
use crate::trace_categories;
use crate::traps;

pub(crate) type JobJoinHandle = tokio::task::JoinHandle<Result<ExecutionResult, error::Error>>;
pub(crate) type JobResult = (Job, Result<ExecutionResult, error::Error>);

/// Manages the jobs that are currently managed by the shell.
#[derive(Default)]
pub struct JobManager {
    /// The jobs that are currently managed by the shell.
    pub jobs: Vec<Job>,
}

/// Represents a task that is part of a job.
pub enum JobTask {
    /// An external process.
    External(processes::ChildProcess),
    /// An internal asynchronous task.
    Internal(JobJoinHandle),
}

/// Represents the result of waiting on a job task.
pub enum JobTaskWaitResult {
    /// The task has completed.
    Completed(ExecutionResult),
    /// The task was stopped.
    Stopped,
}

impl JobTask {
    /// Waits for the task to complete. Returns the result of the wait.
    pub async fn wait(&mut self) -> Result<JobTaskWaitResult, error::Error> {
        match self {
            Self::External(process) => {
                let wait_result = process.wait().await?;
                match wait_result {
                    processes::ProcessWaitResult::Completed(output) => {
                        Ok(JobTaskWaitResult::Completed(output.into()))
                    }
                    processes::ProcessWaitResult::Stopped => Ok(JobTaskWaitResult::Stopped),
                }
            }
            Self::Internal(handle) => Ok(JobTaskWaitResult::Completed(handle.await??)),
        }
    }

    #[allow(clippy::unwrap_in_result)]
    fn poll(&mut self) -> Option<Result<ExecutionResult, error::Error>> {
        match self {
            Self::External(process) => {
                let check_result = process.poll();
                check_result.map(|polled_result| polled_result.map(|output| output.into()))
            }
            Self::Internal(handle) => {
                let checkable_handle = handle;
                checkable_handle.now_or_never().map(|r| r.unwrap())
            }
        }
    }
}

impl JobManager {
    /// Returns a new job manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a job to the job manager and marks it as the current job;
    /// returns an immutable reference to the job.
    ///
    /// # Arguments
    ///
    /// * `job` - The job to add.
    pub fn add_as_current(&mut self, mut job: Job) -> &Job {
        for j in &mut self.jobs {
            if matches!(j.annotation, JobAnnotation::Current) {
                j.annotation = JobAnnotation::Previous;
                break;
            }
        }

        let id = self.jobs.len() + 1;
        job.id = id;
        job.annotation = JobAnnotation::Current;
        self.jobs.push(job);
        self.jobs.last().unwrap()
    }

    /// Returns the current job, if there is one.
    pub fn current_job(&self) -> Option<&Job> {
        self.jobs
            .iter()
            .find(|j| matches!(j.annotation, JobAnnotation::Current))
    }

    /// Returns a mutable reference to the current job, if there is one.
    pub fn current_job_mut(&mut self) -> Option<&mut Job> {
        self.jobs
            .iter_mut()
            .find(|j| matches!(j.annotation, JobAnnotation::Current))
    }

    /// Returns the previous job, if there is one.
    pub fn prev_job(&self) -> Option<&Job> {
        self.jobs
            .iter()
            .find(|j| matches!(j.annotation, JobAnnotation::Previous))
    }

    /// Returns a mutable reference to the previous job, if there is one.
    pub fn prev_job_mut(&mut self) -> Option<&mut Job> {
        self.jobs
            .iter_mut()
            .find(|j| matches!(j.annotation, JobAnnotation::Previous))
    }

    /// Tries to resolve the given job specification to a job.
    ///
    /// # Arguments
    ///
    /// * `job_spec` - The job specification to resolve.
    pub fn resolve_job_spec(&mut self, job_spec: &str) -> Option<&mut Job> {
        let remainder = job_spec.strip_prefix('%')?;

        match remainder {
            "%" | "+" => self.current_job_mut(),
            "-" => self.prev_job_mut(),
            s if s.chars().all(char::is_numeric) => {
                let id = s.parse::<usize>().ok()?;
                self.jobs.iter_mut().find(|j| j.id == id)
            }
            _ => {
                tracing::warn!(target: trace_categories::UNIMPLEMENTED, "unimplemented: job spec naming command: '{job_spec}'");
                None
            }
        }
    }

    /// Waits for all managed jobs to complete.
    pub async fn wait_all(&mut self) -> Result<Vec<Job>, error::Error> {
        for job in &mut self.jobs {
            job.wait().await?;
        }

        Ok(self.sweep_completed_jobs())
    }

    /// Polls all managed jobs for completion.
    pub fn poll(&mut self) -> Result<Vec<JobResult>, error::Error> {
        let mut results = vec![];

        let mut i = 0;
        while i != self.jobs.len() {
            if let Some(result) = self.jobs[i].poll_done()? {
                let job = self.jobs.remove(i);
                results.push((job, result));
            } else if matches!(self.jobs[i].state, JobState::Done) {
                // TODO: This is a workaround to remove jobs that are done but for which we don't
                // know what happened.
                results.push((self.jobs.remove(i), Ok(ExecutionResult::success())));
            } else {
                i += 1;
            }
        }

        Ok(results)
    }

    fn sweep_completed_jobs(&mut self) -> Vec<Job> {
        let mut completed_jobs = vec![];

        let mut i = 0;
        while i != self.jobs.len() {
            if self.jobs[i].tasks.is_empty() {
                completed_jobs.push(self.jobs.remove(i));
            } else {
                i += 1;
            }
        }

        completed_jobs
    }
}

/// Represents the current execution state of a job.
#[derive(Clone)]
pub enum JobState {
    /// Unknown state.
    Unknown,
    /// The job is running.
    Running,
    /// The job is stopped.
    Stopped,
    /// The job has completed.
    Done,
}

impl Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            Self::Running => write!(f, "Running"),
            Self::Stopped => write!(f, "Stopped"),
            Self::Done => write!(f, "Done"),
        }
    }
}

/// Represents an annotation for a job.
#[derive(Clone)]
pub enum JobAnnotation {
    /// No annotation.
    None,
    /// The job is the current job.
    Current,
    /// The job is the previous job.
    Previous,
}

impl Display for JobAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, ""),
            Self::Current => write!(f, "+"),
            Self::Previous => write!(f, "-"),
        }
    }
}

/// Encapsulates a set of processes managed by the shell as a single unit.
pub struct Job {
    /// The tasks that make up the job.
    tasks: VecDeque<JobTask>,

    /// If available, the process group ID of the job's processes.
    pgid: Option<sys::process::ProcessId>,

    /// The annotation of the job (e.g., current, previous).
    annotation: JobAnnotation,

    /// The shell-internal ID of the job.
    pub id: usize,

    /// The command line of the job.
    pub command_line: String,

    /// The current operational state of the job.
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
    /// Returns a new job object.
    ///
    /// # Arguments
    ///
    /// * `children` - The job's known child processes.
    /// * `command_line` - The command line of the job.
    /// * `state` - The current operational state of the job.
    pub(crate) fn new<I>(tasks: I, command_line: String, state: JobState) -> Self
    where
        I: IntoIterator<Item = JobTask>,
    {
        Self {
            id: 0,
            tasks: tasks.into_iter().collect(),
            pgid: None,
            annotation: JobAnnotation::None,
            command_line,
            state,
        }
    }

    /// Returns a pid-style string for the job.
    pub fn to_pid_style_string(&self) -> String {
        let display_pid = self
            .representative_pid()
            .map_or_else(|| String::from("<pid unknown>"), |pid| pid.to_string());
        std::format!("[{}]{}\t{}", self.id, self.annotation, display_pid)
    }

    /// Returns the annotation of the job.
    pub fn annotation(&self) -> JobAnnotation {
        self.annotation.clone()
    }

    /// Returns the command name of the job.
    pub fn command_name(&self) -> &str {
        self.command_line
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default()
    }

    /// Returns whether the job is the current job.
    pub const fn is_current(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Current)
    }

    /// Returns whether the job is the previous job.
    pub const fn is_prev(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Previous)
    }

    /// Polls whether the job has completed.
    pub fn poll_done(
        &mut self,
    ) -> Result<Option<Result<ExecutionResult, error::Error>>, error::Error> {
        let mut result: Option<Result<ExecutionResult, error::Error>> = None;

        tracing::debug!(target: trace_categories::JOBS, "Polling job {} for completion...", self.id);

        while !self.tasks.is_empty() {
            let task = &mut self.tasks[0];
            match task.poll() {
                Some(r) => {
                    self.tasks.remove(0);
                    result = Some(r);
                }
                None => {
                    return Ok(None);
                }
            }
        }

        tracing::debug!(target: trace_categories::JOBS, "Job {} has completed.", self.id);

        self.state = JobState::Done;

        Ok(result)
    }

    /// Waits for the job to complete.
    pub async fn wait(&mut self) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        while let Some(task) = self.tasks.back_mut() {
            match task.wait().await? {
                JobTaskWaitResult::Completed(execution_result) => {
                    result = execution_result;
                    self.tasks.pop_back();
                }
                JobTaskWaitResult::Stopped => {
                    self.state = JobState::Stopped;
                    return Ok(ExecutionResult::stopped());
                }
            }
        }

        self.state = JobState::Done;

        Ok(result)
    }

    /// Moves the job to execute in the background.
    pub fn move_to_background(&mut self) -> Result<(), error::Error> {
        if matches!(self.state, JobState::Stopped) {
            if let Some(pgid) = self.process_group_id() {
                sys::signal::continue_process(pgid)?;
                self.state = JobState::Running;
                Ok(())
            } else {
                Err(error::ErrorKind::FailedToSendSignal.into())
            }
        } else {
            error::unimp("move job to background")
        }
    }

    /// Moves the job to execute in the foreground.
    pub fn move_to_foreground(&mut self) -> Result<(), error::Error> {
        if matches!(self.state, JobState::Stopped) {
            if let Some(pgid) = self.process_group_id() {
                sys::signal::continue_process(pgid)?;
                self.state = JobState::Running;
            } else {
                return Err(error::ErrorKind::FailedToSendSignal.into());
            }
        }

        if let Some(pgid) = self.process_group_id() {
            sys::terminal::move_to_foreground(pgid)?;
        }

        Ok(())
    }

    /// Kills the job.
    ///
    /// # Arguments
    ///
    /// * `signal` - The signal to send to the job.
    pub fn kill(&self, signal: traps::TrapSignal) -> Result<(), error::Error> {
        if let Some(pid) = self.process_group_id() {
            sys::signal::kill_process(pid, signal)
        } else {
            Err(error::ErrorKind::FailedToSendSignal.into())
        }
    }

    /// Tries to retrieve a "representative" pid for the job.
    pub fn representative_pid(&self) -> Option<sys::process::ProcessId> {
        for task in &self.tasks {
            match task {
                JobTask::External(p) => {
                    if let Some(pid) = p.pid() {
                        return Some(pid);
                    }
                }
                JobTask::Internal(_) => (),
            }
        }
        None
    }

    /// Tries to retrieve the process group ID (PGID) of the job.
    pub fn process_group_id(&self) -> Option<sys::process::ProcessId> {
        // TODO: Don't assume that the first PID is the PGID.
        self.pgid.or_else(|| self.representative_pid())
    }
}
