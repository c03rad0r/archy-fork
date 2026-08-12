use anyhow::Result;

use crate::Router;

/// Thin wrappers around `uci` CLI commands over SSH.
impl Router {
    /// `uci get <key>` — returns trimmed value.
    pub fn uci_get(&self, key: &str) -> Result<String> {
        let out = self.run_ok(&format!("uci get {}", key))?;
        Ok(out.trim().to_string())
    }

    /// `uci set <key>=<value>`
    pub fn uci_set(&self, key: &str, value: &str) -> Result<()> {
        self.run_ok(&format!("uci set {}={}", key, shell_quote(value)))?;
        Ok(())
    }

    /// `uci add <config> <type>` — returns the new section name.
    pub fn uci_add(&self, config: &str, section_type: &str) -> Result<String> {
        let out = self.run_ok(&format!("uci add {} {}", config, section_type))?;
        Ok(out.trim().to_string())
    }

    /// `uci add_list <key>=<value>`
    pub fn uci_add_list(&self, key: &str, value: &str) -> Result<()> {
        self.run_ok(&format!("uci add_list {}={}", key, shell_quote(value)))?;
        Ok(())
    }

    /// `uci delete <key>`
    pub fn uci_delete(&self, key: &str) -> Result<()> {
        self.run_ok(&format!("uci delete {}", key))?;
        Ok(())
    }

    /// `uci commit [<config>]`
    pub fn uci_commit(&self, config: Option<&str>) -> Result<()> {
        match config {
            Some(c) => self.run_ok(&format!("uci commit {}", c))?,
            None => self.run_ok("uci commit")?,
        };
        Ok(())
    }

    /// Batch: apply a list of `(key, value)` pairs then commit the config.
    pub fn uci_apply(&self, config: &str, pairs: &[(&str, &str)]) -> Result<()> {
        // `uci set config.section=type` fails with "Entry not found" if
        // /etc/config/<config> doesn't exist yet — true for any config file
        // shipped by the base system (wireless, network, dhcp, ...) but not
        // for a package-defined namespace like "tollgate" that nothing has
        // created a default for. `touch` is a no-op if it already exists.
        self.run_ok(&format!("touch /etc/config/{}", config))?;
        for (key, value) in pairs {
            self.uci_set(key, value)?;
        }
        self.uci_commit(Some(config))?;
        Ok(())
    }
}

/// Wrap a value in single quotes, escaping any embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
