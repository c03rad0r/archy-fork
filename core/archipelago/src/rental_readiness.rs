//! Bounded background verification. Preparing/ready never creates a lease.
use crate::rental_chunk_index::{Binding, Index};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Instant,
};
use tokio::sync::Semaphore;
const MAX_JOBS: usize = 8;
const MAX_INDEX_MEMORY: u64 = 64 * 1024 * 1024;
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum Status {
    Preparing {
        completed_bytes: u64,
        total_bytes: u64,
    },
    Ready {
        ready_id: String,
        total_bytes: u64,
    },
    Unavailable,
}
struct Reservation {
    used: Arc<AtomicU64>,
    bytes: u64,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.used.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}
pub(crate) struct Ready {
    pub index: Index,
    pub id: String,
    _memory: Reservation,
}
#[cfg(test)]
impl Ready {
    pub(crate) fn fixture(index: Index) -> Arc<Self> {
        Arc::new(Self {
            index,
            id: uuid::Uuid::new_v4().to_string(),
            _memory: Reservation {
                used: Arc::new(AtomicU64::new(0)),
                bytes: 0,
            },
        })
    }
}
enum State {
    Preparing,
    Ready(Arc<Ready>),
    Failed,
}
struct Job {
    binding: Binding,
    state: Mutex<State>,
    progress: Arc<AtomicU64>,
    touched: Instant,
}
pub(crate) struct Manager {
    jobs: Mutex<HashMap<String, Arc<Job>>>,
    workers: Arc<Semaphore>,
    memory: Arc<AtomicU64>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
            workers: Arc::new(Semaphore::new(2)),
            memory: Arc::new(AtomicU64::new(0)),
        }
    }
}
pub(crate) fn shared() -> &'static Manager {
    static INSTANCE: OnceLock<Manager> = OnceLock::new();
    INSTANCE.get_or_init(Manager::default)
}
impl Manager {
    #[cfg(test)]
    pub(crate) fn forget_for_restart(&self, key: &str) {
        self.jobs.lock().unwrap().remove(key);
    }

    fn status(job: &Job) -> Result<Status> {
        Ok(
            match &*job
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Media readiness unavailable"))?
            {
                State::Preparing => Status::Preparing {
                    completed_bytes: job.progress.load(Ordering::SeqCst),
                    total_bytes: job.binding.size,
                },
                State::Ready(ready) => Status::Ready {
                    ready_id: ready.id.clone(),
                    total_bytes: job.binding.size,
                },
                State::Failed => Status::Unavailable,
            },
        )
    }
    /// Work closure opens only the already-authorized immutable snapshot. At most
    /// two closures execute, eight jobs exist, and64MiB is reserved for indexes.
    /// Stream-held Arcs retain their reservation even after cache eviction.
    pub fn prepare(
        &self,
        key: String,
        binding: Binding,
        work: impl FnOnce(Arc<AtomicU64>) -> Result<Index> + Send + 'static,
    ) -> Result<Status> {
        let mut jobs = self
            .jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("Media readiness unavailable"))?;
        if let Some(job) = jobs.get(&key) {
            anyhow::ensure!(job.binding == binding, "Readiness immutable terms changed");
            return Self::status(job);
        }
        let bytes = binding
            .index_bytes()?
            .checked_mul(2)
            .and_then(|v| v.checked_add(128 * 1024))
            .context("Index budget overflow")?;
        loop {
            let used = self.memory.load(Ordering::SeqCst);
            if jobs.len() < MAX_JOBS
                && used
                    .checked_add(bytes)
                    .is_some_and(|n| n <= MAX_INDEX_MEMORY)
            {
                break;
            }
            let victim = jobs
                .iter()
                .filter(|(_, job)| {
                    job.state
                        .lock()
                        .map(|state| !matches!(*state, State::Preparing))
                        .unwrap_or(false)
                })
                .min_by_key(|(_, job)| job.touched)
                .map(|(key, _)| key.clone());
            if let Some(victim) = victim {
                jobs.remove(&victim);
            } else {
                return Ok(Status::Preparing {
                    completed_bytes: 0,
                    total_bytes: binding.size,
                });
            }
        }
        let permit = match self.workers.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                return Ok(Status::Preparing {
                    completed_bytes: 0,
                    total_bytes: binding.size,
                })
            }
        };
        // Admissions are serialized by jobs; concurrent drops can only lower use.
        self.memory.fetch_add(bytes, Ordering::SeqCst);
        let reservation = Reservation {
            used: self.memory.clone(),
            bytes,
        };
        let progress = Arc::new(AtomicU64::new(0));
        let job = Arc::new(Job {
            binding: binding.clone(),
            state: Mutex::new(State::Preparing),
            progress: progress.clone(),
            touched: Instant::now(),
        });
        jobs.insert(key, job.clone());
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(progress)))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Media verification worker failed")));
            let mut state = match job.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            *state = match result {
                Ok(index) if index.binding == binding => {
                    job.progress.store(binding.size, Ordering::SeqCst);
                    State::Ready(Arc::new(Ready {
                        index,
                        id: uuid::Uuid::new_v4().to_string(),
                        _memory: reservation,
                    }))
                }
                _ => State::Failed,
            };
        });
        Ok(Status::Preparing {
            completed_bytes: 0,
            total_bytes: binding.size,
        })
    }
    /// Explicit retry clears only a failed preparation, never a running job or
    /// a lease. The caller has already reauthenticated the original purchase.
    pub fn retry_failed(&self, key: &str, binding: &Binding) -> Result<()> {
        let mut jobs = self
            .jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("Media readiness unavailable"))?;
        if let Some(job) = jobs.get(key) {
            anyhow::ensure!(&job.binding == binding, "Readiness immutable terms changed");
            let failed = matches!(
                *job.state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Media readiness unavailable"))?,
                State::Failed
            );
            if failed {
                jobs.remove(key);
            }
        }
        Ok(())
    }
    pub fn ready(
        &self,
        key: &str,
        binding: &Binding,
        ready_id: Option<&str>,
    ) -> Result<Arc<Ready>> {
        let jobs = self
            .jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("Media readiness unavailable"))?;
        let job = jobs
            .get(key)
            .context("Media is preparing; no rental was started")?;
        anyhow::ensure!(&job.binding == binding, "Readiness immutable terms changed");
        let state = job
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Media readiness unavailable"))?;
        let State::Ready(ready) = &*state else {
            anyhow::bail!("Media is not ready; no rental was started");
        };
        anyhow::ensure!(
            ready_id.is_none_or(|id| id == ready.id),
            "Media readiness changed; prepare again without paying"
        );
        Ok(ready.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[tokio::test]
    async fn slow_verification_is_nonblocking_deduplicated_and_has_no_start_side_effect() {
        let manager = Manager::default();
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("media");
        let bytes = vec![9; 1024 * 1024];
        std::fs::write(&media, &bytes).unwrap();
        let binding = Binding {
            content_id: format!("registered_{}", uuid::Uuid::new_v4()),
            receipt_sha256: "ab".repeat(32),
            full_sha256: hex::encode(Sha256::digest(&bytes)),
            size: bytes.len() as u64,
        };
        let (release, wait) = std::sync::mpsc::channel();
        let clone = binding.clone();
        assert!(matches!(
            manager
                .prepare("key".into(), binding.clone(), move |progress| {
                    wait.recv().unwrap();
                    let mut file = std::fs::File::open(media)?;
                    Index::scan(&mut file, clone, |n| {
                        progress.store(n, Ordering::SeqCst);
                        Ok(())
                    })
                })
                .unwrap(),
            Status::Preparing { .. }
        ));
        assert!(manager.ready("key", &binding, None).is_err());
        assert!(matches!(
            manager
                .prepare("key".into(), binding.clone(), |_| panic!(
                    "Duplicate full scan"
                ))
                .unwrap(),
            Status::Preparing { .. }
        ));
        release.send(()).unwrap();
        let ready = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Ok(ready) = manager.ready("key", &binding, None) {
                    break ready;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(manager.ready("key", &binding, Some("other-ready")).is_err());
        assert!(manager.ready("key", &binding, Some(&ready.id)).is_ok());
        let restarted = Manager::default();
        assert!(restarted.ready("key", &binding, Some(&ready.id)).is_err());
    }
}
