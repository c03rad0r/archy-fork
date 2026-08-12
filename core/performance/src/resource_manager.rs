// Resource management and optimization for containers
// Handles CPU, memory, and disk I/O limits and optimization

use anyhow::Result;
use std::collections::HashMap;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub cpu_cores: f64,
    pub memory_mb: u32,
    pub disk_io_read_mbps: Option<u32>,
    pub disk_io_write_mbps: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SystemResources {
    pub total_cpu_cores: u32,
    pub total_memory_mb: u32,
    pub available_disk_gb: u32,
}

pub struct ResourceManager {
    system_resources: SystemResources,
    allocated_resources: HashMap<String, ResourceLimits>,
}

impl ResourceManager {
    pub fn new(system_resources: SystemResources) -> Self {
        Self {
            system_resources,
            allocated_resources: HashMap::new(),
        }
    }

    /// Check if resources are available for a new container
    pub fn can_allocate(&self, requested: &ResourceLimits) -> Result<bool> {
        let mut used_cpu = 0.0;
        let mut used_memory = 0;

        for limits in self.allocated_resources.values() {
            used_cpu += limits.cpu_cores;
            used_memory += limits.memory_mb;
        }

        let available_cpu = self.system_resources.total_cpu_cores as f64 - used_cpu;
        let available_memory = self.system_resources.total_memory_mb - used_memory;

        if requested.cpu_cores > available_cpu {
            return Ok(false);
        }

        if requested.memory_mb > available_memory {
            return Ok(false);
        }

        Ok(true)
    }

    /// Allocate resources for a container
    pub fn allocate(&mut self, container_id: String, limits: ResourceLimits) -> Result<()> {
        if !self.can_allocate(&limits)? {
            return Err(anyhow::anyhow!("Insufficient resources"));
        }

        self.allocated_resources.insert(container_id, limits);
        info!("Allocated resources for container");
        Ok(())
    }

    /// Release resources for a container
    pub fn release(&mut self, container_id: &str) {
        self.allocated_resources.remove(container_id);
        info!("Released resources for container: {}", container_id);
    }

    /// Get current resource usage
    pub fn get_usage(&self) -> (f64, u32) {
        let cpu: f64 = self.allocated_resources.values().map(|r| r.cpu_cores).sum();
        let memory: u32 = self.allocated_resources.values().map(|r| r.memory_mb).sum();
        (cpu, memory)
    }

    /// Optimize resource allocation (reduce limits for low-priority containers)
    pub fn optimize_allocation(&mut self) {
        // TODO: Implement dynamic resource adjustment based on usage
        info!("Optimizing resource allocation");
    }
}
