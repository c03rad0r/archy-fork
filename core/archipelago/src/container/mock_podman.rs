//! Mock container runtime for unit testing orchestration logic.
//!
//! Simulates podman behavior in-memory: container lifecycle, health checks,
//! image pulls (with configurable failures for retry testing).

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};

/// Container state matching podman's real states.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum MockContainerState {
    Created,
    Running,
    Exited(i32), // exit code
    Stopped,
}

impl MockContainerState {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Exited(_) => "exited",
            Self::Stopped => "stopped",
        }
    }
}

/// A simulated container.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MockContainer {
    pub name: String,
    pub image: String,
    pub state: MockContainerState,
    pub stop_timeout_used: Option<u64>,
}

/// Mock podman runtime for testing orchestration logic without real containers.
#[allow(dead_code)]
pub struct MockPodman {
    containers: Arc<Mutex<HashMap<String, MockContainer>>>,
    /// When true, `podman pull` will fail (simulates registry down).
    pub fail_pull: Arc<AtomicBool>,
    /// When true, containers exit immediately after start (simulates crash).
    pub fail_start: Arc<AtomicBool>,
    /// Count of pull attempts (for retry testing).
    pub pull_attempt_count: Arc<AtomicU32>,
    /// Count of start attempts.
    pub start_attempt_count: Arc<AtomicU32>,
    /// Images that have been "pulled" (exist locally).
    images: Arc<Mutex<Vec<String>>>,
}

#[allow(dead_code)]
impl MockPodman {
    pub fn new() -> Self {
        Self {
            containers: Arc::new(Mutex::new(HashMap::new())),
            fail_pull: Arc::new(AtomicBool::new(false)),
            fail_start: Arc::new(AtomicBool::new(false)),
            pull_attempt_count: Arc::new(AtomicU32::new(0)),
            start_attempt_count: Arc::new(AtomicU32::new(0)),
            images: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Simulate `podman pull <image>`. Respects fail_pull flag.
    pub fn pull_image(&self, image: &str) -> Result<(), String> {
        self.pull_attempt_count.fetch_add(1, Ordering::SeqCst);
        if self.fail_pull.load(Ordering::SeqCst) {
            return Err(format!(
                "Error: initializing source docker://{}: connection refused",
                image
            ));
        }
        self.images.lock().unwrap().push(image.to_string());
        Ok(())
    }

    /// Check if an image exists locally (was pulled).
    pub fn image_exists(&self, image: &str) -> bool {
        self.images.lock().unwrap().iter().any(|i| i == image)
    }

    /// Simulate `podman run -d --name <name> <image>`.
    pub fn create_and_start(&self, name: &str, image: &str) -> Result<String, String> {
        self.start_attempt_count.fetch_add(1, Ordering::SeqCst);

        if !self.image_exists(image) {
            return Err(format!("Error: {} not found", image));
        }

        let state = if self.fail_start.load(Ordering::SeqCst) {
            MockContainerState::Exited(1)
        } else {
            MockContainerState::Running
        };

        let container = MockContainer {
            name: name.to_string(),
            image: image.to_string(),
            state,
            stop_timeout_used: None,
        };

        self.containers
            .lock()
            .unwrap()
            .insert(name.to_string(), container);
        Ok(format!("abc123def456_{}", name))
    }

    /// Simulate `podman start <name>`.
    pub fn start(&self, name: &str) -> Result<(), String> {
        let mut containers = self.containers.lock().unwrap();
        match containers.get_mut(name) {
            Some(c) => {
                if self.fail_start.load(Ordering::SeqCst) {
                    c.state = MockContainerState::Exited(1);
                } else {
                    c.state = MockContainerState::Running;
                }
                Ok(())
            }
            None => Err(format!("Error: no such container {}", name)),
        }
    }

    /// Simulate `podman stop -t <timeout> <name>`.
    pub fn stop(&self, name: &str, timeout: u64) -> Result<(), String> {
        let mut containers = self.containers.lock().unwrap();
        match containers.get_mut(name) {
            Some(c) => {
                c.state = MockContainerState::Stopped;
                c.stop_timeout_used = Some(timeout);
                Ok(())
            }
            None => Err(format!("Error: no such container {}", name)),
        }
    }

    /// Simulate `podman rm -f <name>`.
    pub fn remove(&self, name: &str) -> Result<(), String> {
        self.containers.lock().unwrap().remove(name);
        Ok(())
    }

    /// Simulate `podman inspect <name> --format {{.State.Status}}`.
    pub fn inspect_state(&self, name: &str) -> Option<String> {
        self.containers
            .lock()
            .unwrap()
            .get(name)
            .map(|c| c.state.as_str().to_string())
    }

    /// List all containers (like `podman ps -a`).
    pub fn list_all(&self) -> Vec<MockContainer> {
        self.containers.lock().unwrap().values().cloned().collect()
    }

    /// Get a specific container.
    pub fn get(&self, name: &str) -> Option<MockContainer> {
        self.containers.lock().unwrap().get(name).cloned()
    }

    /// Pre-load an image (as if it was already pulled or bundled).
    pub fn preload_image(&self, image: &str) {
        self.images.lock().unwrap().push(image.to_string());
    }

    /// Pre-load a container in a specific state.
    pub fn preload_container(&self, name: &str, image: &str, state: MockContainerState) {
        self.containers.lock().unwrap().insert(
            name.to_string(),
            MockContainer {
                name: name.to_string(),
                image: image.to_string(),
                state,
                stop_timeout_used: None,
            },
        );
    }

    /// Get the stop timeout that was used for a container.
    pub fn get_stop_timeout(&self, name: &str) -> Option<u64> {
        self.containers
            .lock()
            .unwrap()
            .get(name)
            .and_then(|c| c.stop_timeout_used)
    }

    /// Reset all counters and state.
    pub fn reset(&self) {
        self.containers.lock().unwrap().clear();
        self.images.lock().unwrap().clear();
        self.fail_pull.store(false, Ordering::SeqCst);
        self.fail_start.store(false, Ordering::SeqCst);
        self.pull_attempt_count.store(0, Ordering::SeqCst);
        self.start_attempt_count.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pull_and_start() {
        let mock = MockPodman::new();
        mock.pull_image("test:latest").unwrap();
        assert!(mock.image_exists("test:latest"));
        mock.create_and_start("test-container", "test:latest")
            .unwrap();
        assert_eq!(
            mock.inspect_state("test-container"),
            Some("running".to_string())
        );
    }

    #[test]
    fn test_pull_failure() {
        let mock = MockPodman::new();
        mock.fail_pull.store(true, Ordering::SeqCst);
        assert!(mock.pull_image("test:latest").is_err());
        assert!(!mock.image_exists("test:latest"));
        assert_eq!(mock.pull_attempt_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_start_failure() {
        let mock = MockPodman::new();
        mock.preload_image("test:latest");
        mock.fail_start.store(true, Ordering::SeqCst);
        mock.create_and_start("crasher", "test:latest").unwrap();
        assert_eq!(mock.inspect_state("crasher"), Some("exited".to_string()));
    }

    #[test]
    fn test_stop_records_timeout() {
        let mock = MockPodman::new();
        mock.preload_image("test:latest");
        mock.create_and_start("test", "test:latest").unwrap();
        mock.stop("test", 600).unwrap();
        assert_eq!(mock.get_stop_timeout("test"), Some(600));
        assert_eq!(mock.inspect_state("test"), Some("stopped".to_string()));
    }

    #[test]
    fn test_remove() {
        let mock = MockPodman::new();
        mock.preload_image("test:latest");
        mock.create_and_start("removeme", "test:latest").unwrap();
        mock.remove("removeme").unwrap();
        assert!(mock.inspect_state("removeme").is_none());
    }

    #[test]
    fn test_start_without_image_fails() {
        let mock = MockPodman::new();
        assert!(mock.create_and_start("nope", "missing:latest").is_err());
    }

    #[test]
    fn test_preload_container() {
        let mock = MockPodman::new();
        mock.preload_container("existing", "img:1.0", MockContainerState::Running);
        assert_eq!(mock.inspect_state("existing"), Some("running".to_string()));
        assert_eq!(mock.list_all().len(), 1);
    }

    #[test]
    fn test_reset() {
        let mock = MockPodman::new();
        mock.preload_image("img:1");
        mock.preload_container("c1", "img:1", MockContainerState::Running);
        mock.fail_pull.store(true, Ordering::SeqCst);
        mock.reset();
        assert!(!mock.image_exists("img:1"));
        assert!(mock.list_all().is_empty());
        assert!(!mock.fail_pull.load(Ordering::SeqCst));
    }
}
