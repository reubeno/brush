use anyhow::Result;

use crate::ExecutionResult;

type JobJoinHandle = tokio::task::JoinHandle<Result<ExecutionResult>>;

#[derive(Default)]
pub struct JobManager {
    pub background_jobs: Vec<JobJoinHandle>,
}

impl JobManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, job: JobJoinHandle) -> usize {
        self.background_jobs.push(job);
        self.background_jobs.len()
    }
}
