//! Concurrency limiter for LLM subprocess spawns.
//!
//! Wraps a `tokio::sync::Semaphore` to limit the number of parallel
//! `claude -p` subprocesses. Without this, a codebase with 500 cache
//! misses would fork 500 claude processes simultaneously.

use std::sync::Arc;
use tokio::sync::Semaphore;

/// A bounded concurrency pool for LLM invocations.
#[derive(Clone)]
#[derive(Debug)]
pub struct LlmPool {
    semaphore: Arc<Semaphore>,
}

impl LlmPool {
    /// Create a pool that allows at most `max_concurrent` parallel
    /// LLM subprocess invocations.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Acquire a permit, blocking until one is available. The permit
    /// is released when the returned guard is dropped.
    /// Acquire a permit. Returns `Err` only if the semaphore is closed
    /// (which can't happen in normal comply operation since we own it).
    pub async fn acquire(&self) -> Result<tokio::sync::OwnedSemaphorePermit, anyhow::Error> {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("LLM pool semaphore closed unexpectedly"))
    }
}
