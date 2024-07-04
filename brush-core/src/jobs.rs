use std::collections::VecDeque;
use std::fmt::Display;

use futures::FutureExt;

use crate::error;
use crate::sys;
use crate::ExecutionResult;

pub(crate) type JobJoinHandle = tokio::task::JoinHandle<Result<ExecutionResult, error::Error>>;
pub(crate) type JobResult = (Job, Result<ExecutionResult, error::Error>);

/// Manages the jobs that are currently managed by the shell.
#[derive(Default)]
pub struct JobManager {
    /// The jobs that are currently managed by the shell.
    pub jobs: Vec<Job>,
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
        if !job_spec.starts_with('%') {
            return None;
        }

        match &job_spec[1..] {
            "%" | "+" => self.current_job_mut(),
            "-" => self.prev_job_mut(),
            s if s.chars().all(char::is_numeric) => {
                let id = s.parse::<usize>().ok()?;
                self.jobs.iter_mut().find(|j| j.id == id)
            }
            _ => {
                tracing::warn!("UNIMPLEMENTED: job spec naming command: '{job_spec}'");
                None
            }
        }
    }

    /// Waits for all managede jobs to complete.
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
            if self.jobs[i].join_handles.is_empty() {
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
            JobState::Unknown => write!(f, "Unknown"),
            JobState::Running => write!(f, "Running"),
            JobState::Stopped => write!(f, "Stopped"),
            JobState::Done => write!(f, "Done"),
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
            JobAnnotation::None => write!(f, ""),
            JobAnnotation::Current => write!(f, "+"),
            JobAnnotation::Previous => write!(f, "-"),
        }
    }
}

/// Encapsulates a set of processes managed by the shell as a single unit.
pub struct Job {
    /// Join handles for the tasks that are waiting on the job's processes.
    join_handles: VecDeque<JobJoinHandle>,

    /// If available, the process IDs of the job's processes.
    pids: Vec<u32>,

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
    /// * `join_handles` - The join handles for the tasks that are waiting on the job's processes.
    /// * `pids` - If available, the process IDs of the job's processes.
    /// * `command_line` - The command line of the job.
    /// * `state` - The current operational state of the job.
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

    /// Returns a pid-style string for the job.
    pub fn to_pid_style_string(&self) -> String {
        let display_pid = self
            .get_representative_pid()
            .map_or_else(|| String::from("<pid unknown>"), |pid| pid.to_string());
        std::format!("[{}]{}\t{}", self.id, self.annotation, display_pid)
    }

    /// Returns the annotation of the job.
    pub fn get_annotation(&self) -> JobAnnotation {
        self.annotation.clone()
    }

    /// Returns the command name of the job.
    pub fn get_command_name(&self) -> &str {
        self.command_line
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default()
    }

    /// Returns whether the job is the current job.
    pub fn is_current(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Current)
    }

    /// Returns whether the job is the previous job.
    pub fn is_prev(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Previous)
    }

    /// Polls whether the job has completed.
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

    /// Waits for the job to complete.
    pub async fn wait(&mut self) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        while let Some(join_handle) = self.join_handles.pop_back() {
            result = join_handle.await.unwrap()?;
        }

        Ok(result)
    }

    /// Moves the job to execute in the background.
    #[allow(clippy::unused_self)]
    pub fn move_to_background(&mut self) -> Result<(), error::Error> {
        error::unimp("move job to background")
    }

    /// Moves the job to execute in the foreground.
    pub fn move_to_foreground(&mut self) -> Result<(), error::Error> {
        if !matches!(self.state, JobState::Stopped) {
            return error::unimp("move job to foreground for not stopped job");
        }

        if let Some(pid) = self.get_representative_pid() {
            sys::signal::continue_process(pid)?;
            self.state = JobState::Running;
            Ok(())
        } else {
            Err(error::Error::FailedToSendSignal)
        }
    }

    /// Kills the job.
    pub fn kill(&mut self) -> Result<(), error::Error> {
        if let Some(pid) = self.get_representative_pid() {
            sys::signal::kill_process(pid)
        } else {
            Err(error::Error::FailedToSendSignal)
        }
    }

    /// Tries to retrieve a "representative" pid for the job.
    pub fn get_representative_pid(&self) -> Option<u32> {
        self.pids.first().copied()
    }
}
