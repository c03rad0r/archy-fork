use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PortError {
    #[error("Port {0} is already allocated to app {1}")]
    PortConflict(u16, String),
    #[error("App {0} has no allocated ports")]
    NoPortsAllocated(String),
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),
}

pub struct PortManager {
    allocations: Arc<RwLock<HashMap<String, Vec<u16>>>>,
    port_to_app: Arc<RwLock<HashMap<u16, String>>>,
    port_offset: u16,
}

impl PortManager {
    pub fn new(port_offset: u16) -> Self {
        Self {
            allocations: Arc::new(RwLock::new(HashMap::new())),
            port_to_app: Arc::new(RwLock::new(HashMap::new())),
            port_offset,
        }
    }

    /// Allocate ports for an app, applying the port offset
    pub fn allocate_ports(&self, app_id: &str, base_ports: &[u16]) -> Result<Vec<u16>, PortError> {
        let mut allocations = self
            .allocations
            .write()
            .map_err(|e| PortError::LockPoisoned(e.to_string()))?;
        let mut port_to_app = self
            .port_to_app
            .write()
            .map_err(|e| PortError::LockPoisoned(e.to_string()))?;
        let mut allocated_ports = Vec::new();

        // Check for conflicts and allocate ports
        for &base_port in base_ports {
            let dev_port = base_port + self.port_offset;

            // Check if port is already allocated
            if let Some(existing_app) = port_to_app.get(&dev_port) {
                if existing_app != app_id {
                    return Err(PortError::PortConflict(dev_port, existing_app.clone()));
                }
            }

            allocated_ports.push(dev_port);
            port_to_app.insert(dev_port, app_id.to_string());
        }

        // Store allocation for this app
        allocations.insert(app_id.to_string(), allocated_ports.clone());

        Ok(allocated_ports)
    }

    /// Get allocated ports for an app
    pub fn get_port_mapping(&self, app_id: &str) -> Result<Option<Vec<u16>>, PortError> {
        let allocations = self
            .allocations
            .read()
            .map_err(|e| PortError::LockPoisoned(e.to_string()))?;
        Ok(allocations.get(app_id).cloned())
    }

    /// Get the dev port for a specific base port of an app
    pub fn get_dev_port(&self, app_id: &str, base_port: u16) -> Result<Option<u16>, PortError> {
        Ok(self.get_port_mapping(app_id)?.and_then(|ports| {
            ports
                .iter()
                .find(|&&p| p == base_port + self.port_offset)
                .copied()
        }))
    }

    /// Release all ports allocated to an app
    pub fn release_ports(&self, app_id: &str) -> Result<(), PortError> {
        let mut allocations = self
            .allocations
            .write()
            .map_err(|e| PortError::LockPoisoned(e.to_string()))?;
        let mut port_to_app = self
            .port_to_app
            .write()
            .map_err(|e| PortError::LockPoisoned(e.to_string()))?;

        if let Some(ports) = allocations.remove(app_id) {
            for port in ports {
                port_to_app.remove(&port);
            }
            Ok(())
        } else {
            Err(PortError::NoPortsAllocated(app_id.to_string()))
        }
    }

    /// Check if a port is available
    pub fn is_port_available(&self, base_port: u16) -> Result<bool, PortError> {
        let dev_port = base_port + self.port_offset;
        let port_to_app = self
            .port_to_app
            .read()
            .map_err(|e| PortError::LockPoisoned(e.to_string()))?;
        Ok(!port_to_app.contains_key(&dev_port))
    }

    /// Get all allocated ports
    pub fn get_all_allocations(&self) -> Result<HashMap<String, Vec<u16>>, PortError> {
        let allocations = self
            .allocations
            .read()
            .map_err(|e| PortError::LockPoisoned(e.to_string()))?;
        Ok(allocations.clone())
    }

    /// Get port offset
    pub fn port_offset(&self) -> u16 {
        self.port_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_allocation() {
        let manager = PortManager::new(10000);

        let ports = manager.allocate_ports("app1", &[8332, 8333]).unwrap();
        assert_eq!(ports, vec![18332, 18333]);

        let mapping = manager.get_port_mapping("app1").unwrap().unwrap();
        assert_eq!(mapping, vec![18332, 18333]);
    }

    #[test]
    fn test_port_conflict() {
        let manager = PortManager::new(10000);

        manager.allocate_ports("app1", &[8332]).unwrap();

        // Try to allocate the same port to another app
        let result = manager.allocate_ports("app2", &[8332]);
        assert!(result.is_err());
    }

    #[test]
    fn test_port_release() {
        let manager = PortManager::new(10000);

        manager.allocate_ports("app1", &[8332]).unwrap();
        manager.release_ports("app1").unwrap();

        // Port should now be available
        assert!(manager.is_port_available(8332).unwrap());
    }

    #[test]
    fn test_get_dev_port() {
        let manager = PortManager::new(10000);

        manager.allocate_ports("app1", &[8332, 8333]).unwrap();

        assert_eq!(manager.get_dev_port("app1", 8332).unwrap(), Some(18332));
        assert_eq!(manager.get_dev_port("app1", 8333).unwrap(), Some(18333));
        assert_eq!(manager.get_dev_port("app1", 9999).unwrap(), None);
    }
}
