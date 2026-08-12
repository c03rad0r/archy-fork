// AppArmor/SELinux policy generator for containers
// Creates security profiles for each containerized app

use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;

pub struct ContainerPolicyGenerator {
    policies_dir: PathBuf,
}

impl ContainerPolicyGenerator {
    pub fn new(policies_dir: PathBuf) -> Self {
        Self { policies_dir }
    }

    /// Generate AppArmor profile for a container
    pub async fn generate_apparmor_profile(
        &self,
        app_id: &str,
        capabilities: &[String],
        readonly: bool,
    ) -> Result<PathBuf> {
        let profile_path = self.policies_dir.join(format!("{}.apparmor", app_id));

        let mut profile = String::from("# AppArmor profile for Archipelago container\n");
        profile.push_str(&format!(
            "profile archipelago-{} flags=(attach_disconnected,mediate_deleted) {{\n",
            app_id
        ));

        // Base includes
        profile.push_str("  #include <abstractions/base>\n");

        // Capabilities
        if capabilities.is_empty() {
            profile.push_str("  capability,\n");
        } else {
            for cap in capabilities {
                profile.push_str(&format!("  capability {},\n", cap));
            }
        }

        // Filesystem access
        if readonly {
            profile.push_str("  deny / rw,\n");
            profile.push_str(&format!("  /var/lib/archipelago/{} rw,\n", app_id));
        } else {
            profile.push_str("  / r,\n");
            profile.push_str(&format!("  /var/lib/archipelago/{} rw,\n", app_id));
        }

        // Network
        profile.push_str("  network,\n");

        profile.push_str("}\n");

        fs::write(&profile_path, profile).await?;
        Ok(profile_path)
    }

    /// Apply AppArmor profile to a container
    pub async fn apply_profile(&self, _container_name: &str, profile_path: &PathBuf) -> Result<()> {
        // Load the profile
        tokio::process::Command::new("apparmor_parser")
            .arg("-r")
            .arg(profile_path)
            .output()
            .await?;

        // TODO: Configure Podman to use the profile
        // This requires Podman configuration changes

        Ok(())
    }
}
