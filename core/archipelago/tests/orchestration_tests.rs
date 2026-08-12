//! Container orchestration tests.
//!
//! Tests the orchestration LOGIC without real containers:
//! - Stop grace periods per container type
//! - Image pull retry with exponential backoff
//! - Restart tracker persistence across process restarts
//! - Health monitor tier ordering and user-stopped filtering
//! - Crash recovery snapshot loading
//! - Failsafe install verification
//!
//! Self-contained: no imports from the archipelago binary crate.
//! Uses inline mock + duplicated logic functions to test correctness.

#[path = "../src/container/mock_podman.rs"]
mod mock_podman;

// ── Stop Grace Periods ─────────────────────────────────────────────────

mod stop_grace_periods {
    /// Mirror of runtime.rs stop_timeout_secs — kept in sync.
    /// Tests verify the logic; the real function lives in runtime.rs.
    fn stop_timeout_secs(container_name: &str) -> &'static str {
        let id = container_name
            .strip_prefix("archy-")
            .unwrap_or(container_name);
        match id {
            "bitcoin-knots" | "bitcoin-core" | "bitcoin" => "600",
            "lnd" => "330",
            "electrumx" | "electrs" | "mempool-electrs" => "300",
            "btcpay-db" | "mempool-db" | "penpot-postgres" | "immich_postgres" | "nextcloud-db"
            | "endurain-db" => "120",
            "btcpay-server" | "nbxplorer" | "fedimint" | "fedimint-gateway" => "60",
            _ => "30",
        }
    }

    #[test]
    fn bitcoin_core_gets_600s() {
        assert_eq!(stop_timeout_secs("bitcoin-knots"), "600");
        assert_eq!(stop_timeout_secs("bitcoin-core"), "600");
        assert_eq!(stop_timeout_secs("bitcoin"), "600");
    }

    #[test]
    fn bitcoin_with_archy_prefix() {
        assert_eq!(stop_timeout_secs("archy-bitcoin-knots"), "600");
    }

    #[test]
    fn lnd_gets_330s() {
        assert_eq!(stop_timeout_secs("lnd"), "330");
        assert_eq!(stop_timeout_secs("archy-lnd"), "330");
    }

    #[test]
    fn indexers_get_300s() {
        assert_eq!(stop_timeout_secs("electrumx"), "300");
        assert_eq!(stop_timeout_secs("electrs"), "300");
        assert_eq!(stop_timeout_secs("mempool-electrs"), "300");
    }

    #[test]
    fn databases_get_120s() {
        assert_eq!(stop_timeout_secs("btcpay-db"), "120");
        assert_eq!(stop_timeout_secs("archy-mempool-db"), "120");
        assert_eq!(stop_timeout_secs("penpot-postgres"), "120");
        assert_eq!(stop_timeout_secs("immich_postgres"), "120");
    }

    #[test]
    fn btcpay_services_get_60s() {
        assert_eq!(stop_timeout_secs("btcpay-server"), "60");
        assert_eq!(stop_timeout_secs("nbxplorer"), "60");
        assert_eq!(stop_timeout_secs("fedimint"), "60");
    }

    #[test]
    fn default_is_30s() {
        assert_eq!(stop_timeout_secs("grafana"), "30");
        assert_eq!(stop_timeout_secs("filebrowser"), "30");
        assert_eq!(stop_timeout_secs("searxng"), "30");
        assert_eq!(stop_timeout_secs("ollama"), "30");
        assert_eq!(stop_timeout_secs("unknown-app"), "30");
    }

    #[test]
    fn ui_containers_get_30s() {
        assert_eq!(stop_timeout_secs("archy-bitcoin-ui"), "30");
        assert_eq!(stop_timeout_secs("archy-lnd-ui"), "30");
        assert_eq!(stop_timeout_secs("archy-electrs-ui"), "30");
    }
}

// ── Image Pull Retry Logic ─────────────────────────────────────────────

mod pull_retry {
    use crate::mock_podman::MockPodman;
    use std::sync::atomic::Ordering;

    /// Simulate the retry logic from install.rs: 3 attempts, backoff.
    fn pull_with_retry(mock: &MockPodman, image: &str) -> Result<(), String> {
        const MAX_ATTEMPTS: u32 = 3;

        for attempt in 1..=MAX_ATTEMPTS {
            match mock.pull_image(image) {
                Ok(()) => return Ok(()),
                Err(e) if attempt < MAX_ATTEMPTS => {
                    // In real code, we'd sleep here. In tests, just continue.
                    let _ = e;
                }
                Err(e) => return Err(format!("Failed after {} attempts: {}", MAX_ATTEMPTS, e)),
            }
        }
        unreachable!()
    }

    #[test]
    fn succeeds_first_try() {
        let mock = MockPodman::new();
        pull_with_retry(&mock, "test:1.0").unwrap();
        assert_eq!(mock.pull_attempt_count.load(Ordering::SeqCst), 1);
        assert!(mock.image_exists("test:1.0"));
    }

    #[test]
    fn fails_then_succeeds() {
        let mock = MockPodman::new();
        // Simulate: fail attempt 1, succeed attempt 2
        mock.fail_pull.store(true, Ordering::SeqCst);

        // Attempt 1: fails
        assert!(mock.pull_image("test:1.0").is_err());
        assert_eq!(mock.pull_attempt_count.load(Ordering::SeqCst), 1);

        // Registry comes back
        mock.fail_pull.store(false, Ordering::SeqCst);

        // Attempt 2: succeeds
        assert!(mock.pull_image("test:1.0").is_ok());
        assert_eq!(mock.pull_attempt_count.load(Ordering::SeqCst), 2);
        assert!(mock.image_exists("test:1.0"));
    }

    #[test]
    fn all_attempts_fail() {
        let mock = MockPodman::new();
        mock.fail_pull.store(true, Ordering::SeqCst);
        let result = pull_with_retry(&mock, "test:1.0");
        assert!(result.is_err());
        assert_eq!(mock.pull_attempt_count.load(Ordering::SeqCst), 3);
        assert!(!mock.image_exists("test:1.0"));
    }
}

// ── Restart Tracker Persistence ────────────────────────────────────────

mod restart_tracker {
    use std::collections::HashMap;
    use tempfile::TempDir;

    // Inline the serialization structs (same as health_monitor.rs)
    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct RestartHistory {
        containers: HashMap<String, ContainerRestartRecord>,
    }

    #[derive(serde::Serialize, serde::Deserialize, Clone)]
    struct ContainerRestartRecord {
        attempts: u32,
        last_failure_epoch: i64,
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("restart-tracker.json");

        let mut history = RestartHistory::default();
        history.containers.insert(
            "bitcoin-knots".to_string(),
            ContainerRestartRecord {
                attempts: 2,
                last_failure_epoch: 1700000000,
            },
        );
        history.containers.insert(
            "lnd".to_string(),
            ContainerRestartRecord {
                attempts: 1,
                last_failure_epoch: 1700000100,
            },
        );

        // Save
        let json = serde_json::to_string(&history).unwrap();
        std::fs::write(&path, &json).unwrap();

        // Load
        let loaded_json = std::fs::read_to_string(&path).unwrap();
        let loaded: RestartHistory = serde_json::from_str(&loaded_json).unwrap();

        assert_eq!(loaded.containers.len(), 2);
        assert_eq!(loaded.containers["bitcoin-knots"].attempts, 2);
        assert_eq!(loaded.containers["lnd"].attempts, 1);
    }

    #[test]
    fn missing_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("restart-tracker.json");

        let result = std::fs::read_to_string(&path);
        assert!(result.is_err());

        // Same behavior as health_monitor.rs: unwrap_or_default
        let history: RestartHistory = result
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        assert!(history.containers.is_empty());
    }

    #[test]
    fn corrupt_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("restart-tracker.json");
        std::fs::write(&path, "not valid json {{{").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let history: RestartHistory = serde_json::from_str(&content).unwrap_or_default();
        assert!(history.containers.is_empty());
    }

    #[test]
    fn clear_removes_container() {
        let mut history = RestartHistory::default();
        history.containers.insert(
            "test".to_string(),
            ContainerRestartRecord {
                attempts: 3,
                last_failure_epoch: 1700000000,
            },
        );
        history.containers.remove("test");
        assert!(history.containers.is_empty());
    }

    #[test]
    fn stability_window_check() {
        let now = chrono::Utc::now().timestamp();
        let one_hour_ago = now - 3601;
        let five_min_ago = now - 300;

        // Old failure: should reset
        let old_record = ContainerRestartRecord {
            attempts: 3,
            last_failure_epoch: one_hour_ago,
        };
        assert!(now - old_record.last_failure_epoch >= 3600);

        // Recent failure: should NOT reset
        let recent_record = ContainerRestartRecord {
            attempts: 3,
            last_failure_epoch: five_min_ago,
        };
        assert!(now - recent_record.last_failure_epoch < 3600);
    }
}

// ── Failsafe Install ──────────────────────────────────────────────────

mod failsafe_install {
    use crate::mock_podman::MockPodman;
    use std::sync::atomic::Ordering;

    #[test]
    fn successful_install_flow() {
        let mock = MockPodman::new();
        // Pull succeeds
        mock.pull_image("registry/app:1.0").unwrap();
        // Image exists
        assert!(mock.image_exists("registry/app:1.0"));
        // Container starts
        mock.create_and_start("test-app", "registry/app:1.0")
            .unwrap();
        // Running state
        assert_eq!(mock.inspect_state("test-app"), Some("running".to_string()));
    }

    #[test]
    fn rollback_on_immediate_exit() {
        let mock = MockPodman::new();
        mock.preload_image("registry/app:1.0");
        mock.fail_start.store(true, Ordering::SeqCst);

        // Container is created but exits immediately
        mock.create_and_start("crasher", "registry/app:1.0")
            .unwrap();
        assert_eq!(mock.inspect_state("crasher"), Some("exited".to_string()));

        // Rollback: remove the failed container
        mock.remove("crasher").unwrap();
        assert!(mock.inspect_state("crasher").is_none());
    }

    #[test]
    fn no_image_after_pull_is_error() {
        let mock = MockPodman::new();
        // Don't pull — image doesn't exist
        let result = mock.create_and_start("no-image", "missing:1.0");
        assert!(result.is_err());
    }
}

// ── Health Monitor Logic ──────────────────────────────────────────────

mod health_monitor_logic {

    /// Mirrors the tier ordering from health_monitor.rs
    fn container_tier(name: &str) -> u8 {
        let id = name.strip_prefix("archy-").unwrap_or(name);
        match id {
            "btcpay-db" | "mempool-db" | "penpot-postgres" | "immich_postgres" | "immich_redis"
            | "penpot-valkey" | "endurain-db" | "nextcloud-db" => 0,
            "bitcoin-knots" | "bitcoin-core" | "bitcoin" => 1,
            "lnd" | "electrumx" | "mempool-electrs" | "electrs" | "nbxplorer" => 2,
            "mempool-web" | "bitcoin-ui" | "lnd-ui" | "electrs-ui" | "penpot-frontend"
            | "penpot-exporter" => 4,
            _ => 3,
        }
    }

    #[test]
    fn tier_ordering_databases_first() {
        assert!(container_tier("btcpay-db") < container_tier("bitcoin-knots"));
        assert!(container_tier("mempool-db") < container_tier("lnd"));
    }

    #[test]
    fn tier_ordering_core_before_services() {
        assert!(container_tier("bitcoin-knots") < container_tier("lnd"));
        assert!(container_tier("bitcoin-knots") < container_tier("electrumx"));
    }

    #[test]
    fn tier_ordering_services_before_apps() {
        assert!(container_tier("lnd") < container_tier("grafana"));
        assert!(container_tier("electrumx") < container_tier("filebrowser"));
    }

    #[test]
    fn tier_ordering_apps_before_uis() {
        assert!(container_tier("grafana") < container_tier("bitcoin-ui"));
        assert!(container_tier("filebrowser") < container_tier("lnd-ui"));
    }

    #[test]
    fn user_stopped_containers_skipped() {
        let user_stopped: std::collections::HashSet<String> =
            ["archy-grafana".to_string(), "filebrowser".to_string()].into();

        // Simulated unhealthy containers
        let unhealthy = vec!["archy-grafana", "filebrowser", "lnd"];

        let to_restart: Vec<&str> = unhealthy
            .into_iter()
            .filter(|name| !user_stopped.contains(*name))
            .collect();

        assert_eq!(to_restart, vec!["lnd"]);
    }

    #[test]
    fn all_long_running_containers_monitored() {
        // Health monitor now checks ALL containers except ephemeral build/init ones.
        // Backend services and UI containers are monitored for auto-restart.
        let containers = [
            ("bitcoin-knots", "exited"),
            ("archy-bitcoin-ui", "exited"),
            ("archy-lnd-ui", "exited"),
            ("grafana", "exited"),
            ("nbxplorer", "exited"),
            ("indeedhub-build_api_1", "exited"),
            ("btcpay-init", "exited"),
        ];

        let to_check: Vec<&str> = containers
            .iter()
            .filter(|(name, _)| !name.starts_with("indeedhub-build_") && !name.contains("-init"))
            .map(|(name, _)| *name)
            .collect();

        assert_eq!(
            to_check,
            vec![
                "bitcoin-knots",
                "archy-bitcoin-ui",
                "archy-lnd-ui",
                "grafana",
                "nbxplorer",
            ]
        );
    }

    #[test]
    fn restart_sorted_by_tier() {
        let mut unhealthy = vec![
            "grafana",       // tier 3
            "lnd",           // tier 2
            "btcpay-db",     // tier 0
            "bitcoin-knots", // tier 1
        ];

        unhealthy.sort_by_key(|name| container_tier(name));

        assert_eq!(
            unhealthy,
            vec!["btcpay-db", "bitcoin-knots", "lnd", "grafana"]
        );
    }
}

// ── Crash Recovery ────────────────────────────────────────────────────

mod crash_recovery {
    use tempfile::TempDir;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct ContainerSnapshot {
        timestamp: u64,
        containers: Vec<RunningContainerRecord>,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    struct RunningContainerRecord {
        name: String,
        image: String,
    }

    #[test]
    fn snapshot_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("running-containers.json");

        let snapshot = ContainerSnapshot {
            timestamp: 1700000000,
            containers: vec![
                RunningContainerRecord {
                    name: "bitcoin-knots".to_string(),
                    image: "bitcoin-knots:28.1".to_string(),
                },
                RunningContainerRecord {
                    name: "lnd".to_string(),
                    image: "lnd:0.18.5".to_string(),
                },
            ],
        };

        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        std::fs::write(&path, &json).unwrap();

        let loaded_json = std::fs::read_to_string(&path).unwrap();
        let loaded: ContainerSnapshot = serde_json::from_str(&loaded_json).unwrap();

        assert_eq!(loaded.containers.len(), 2);
        assert_eq!(loaded.containers[0].name, "bitcoin-knots");
    }

    #[test]
    fn user_stopped_filtering() {
        let user_stopped: std::collections::HashSet<String> = ["grafana".to_string()].into();

        let snapshot_containers = [
            "bitcoin-knots".to_string(),
            "lnd".to_string(),
            "grafana".to_string(),
        ];

        let to_recover: Vec<&String> = snapshot_containers
            .iter()
            .filter(|name| !user_stopped.contains(name.as_str()))
            .collect();

        assert_eq!(to_recover.len(), 2);
        assert!(!to_recover.iter().any(|n| n.as_str() == "grafana"));
    }

    #[test]
    fn boot_tier_ordering() {
        fn boot_tier(name: &str) -> u8 {
            let id = name.strip_prefix("archy-").unwrap_or(name);
            match id {
                "btcpay-db" | "mempool-db" => 0,
                "bitcoin-knots" | "bitcoin-core" => 1,
                "lnd" | "electrumx" => 2,
                "mempool-web" | "bitcoin-ui" | "lnd-ui" => 4,
                _ => 3,
            }
        }

        let mut containers = [
            "mempool-web",
            "lnd",
            "btcpay-db",
            "bitcoin-knots",
            "grafana",
        ];

        containers.sort_by_key(|name| boot_tier(name));

        assert_eq!(containers[0], "btcpay-db");
        assert_eq!(containers[1], "bitcoin-knots");
        assert_eq!(containers[2], "lnd");
        assert_eq!(containers[3], "grafana");
        assert_eq!(containers[4], "mempool-web");
    }
}
