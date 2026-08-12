//! How long a login lasts, and who gets to say so.
//!
//! # Why this is configurable rather than a constant
//!
//! There is no single correct session lifetime. The same node can be a
//! wall-mounted TV in a living room that must never ask for a password
//! mid-film, and a wallet holding real funds where PCI DSS-style guidance
//! says fifteen minutes. Both are legitimate; the operator knows which one
//! this node is and we do not.
//!
//! # The two tokens
//!
//! * **Session token** — short-lived, refreshed silently on every
//!   authenticated request. This is what the browser sends; if it leaks, it
//!   is useful only until [`SessionPolicy::idle_timeout_secs`] of silence.
//! * **Login (remember) token** — long-lived, and its *only* power is to
//!   mint a fresh session token. Kept separate so raising the convenience
//!   knob does not put a 30-day bearer credential on every request.
//!
//! Raising the idle timeout therefore does not weaken the credential that
//! actually travels; it only changes how long a quiet tab stays usable.
//!
//! # Why an absolute cap exists at all
//!
//! Idle timeout alone can be defeated by any page that polls — the
//! dashboard polls constantly, so an idle timeout would never fire while a
//! tab is open. The absolute cap is what guarantees a login eventually
//! ends, which is the property an auditor actually asks about.

use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE_PATH: &str = "settings/session_policy.json";

/// Bounds. A setting that can be made meaningless is not a setting, and one
/// that can lock the operator out of their own node is a footgun.
const MIN_IDLE_SECS: u64 = 60;
const MAX_IDLE_SECS: u64 = 90 * 24 * 3600;
const MIN_ABSOLUTE_SECS: u64 = 300;
const MAX_ABSOLUTE_SECS: u64 = 365 * 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceClass {
    /// Ordinary browser on a phone or laptop. Policy applies as configured.
    Browser,
    /// A screen nobody logs into — a wall-mounted dashboard or TV. Being
    /// signed out mid-view is the failure mode here, not a stale session:
    /// the device is physically in the home, and there is no keyboard to
    /// re-authenticate with. Exempt from the idle timeout, still subject to
    /// the absolute cap so a stolen box does not stay authenticated forever.
    Kiosk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPolicy {
    /// Silence after which a session token stops validating.
    pub idle_timeout_secs: u64,
    /// Hard ceiling from login, regardless of activity. `None` = no cap.
    pub absolute_timeout_secs: Option<u64>,
    /// Re-prompt for the password before actions that move money, however
    /// fresh the session is. Independent of the timeouts on purpose: it is
    /// the control that matters when funds are involved, and it costs the
    /// operator nothing the rest of the time.
    pub reauth_for_funds: bool,
}

impl Default for SessionPolicy {
    fn default() -> Self {
        Self {
            // A day of silence, matching the previous hard-coded constant so
            // existing nodes see no behaviour change until someone chooses.
            idle_timeout_secs: 86_400,
            // 30 days, aligned with the login token's own lifetime: a
            // session that outlived the token which could refresh it would
            // be an oddity.
            absolute_timeout_secs: Some(30 * 24 * 3600),
            reauth_for_funds: true,
        }
    }
}

impl SessionPolicy {
    /// Clamp to the supported range. Applied on load as well as on save, so
    /// a hand-edited file cannot disable expiry by writing `0`.
    pub fn sanitized(mut self) -> Self {
        self.idle_timeout_secs = self.idle_timeout_secs.clamp(MIN_IDLE_SECS, MAX_IDLE_SECS);
        self.absolute_timeout_secs = self
            .absolute_timeout_secs
            .map(|v| v.clamp(MIN_ABSOLUTE_SECS, MAX_ABSOLUTE_SECS))
            // An absolute cap below the idle timeout would expire sessions
            // while they are still active, which reads as random logouts.
            .map(|v| v.max(self.idle_timeout_secs));
        self
    }

    /// Idle timeout for a given device, or `None` when idleness is not a
    /// reason to expire (kiosk screens).
    pub fn idle_timeout_for(&self, class: DeviceClass) -> Option<u64> {
        match class {
            DeviceClass::Browser => Some(self.idle_timeout_secs),
            DeviceClass::Kiosk => None,
        }
    }

    /// Has a session expired? `age` is time since login, `idle` since last
    /// use. Both are checked because either alone is insufficient: idle
    /// never fires on a polling dashboard, and absolute alone leaves a
    /// forgotten tab usable for a month.
    pub fn is_expired(&self, class: DeviceClass, age_secs: u64, idle_secs: u64) -> bool {
        if let Some(limit) = self.absolute_timeout_secs {
            if age_secs >= limit {
                return true;
            }
        }
        match self.idle_timeout_for(class) {
            Some(limit) => idle_secs >= limit,
            None => false,
        }
    }
}

pub async fn load(data_dir: &Path) -> SessionPolicy {
    let path = data_dir.join(FILE_PATH);
    match tokio::fs::read(&path).await {
        Ok(bytes) => serde_json::from_slice::<SessionPolicy>(&bytes)
            .map(SessionPolicy::sanitized)
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "session policy unreadable; using defaults");
                SessionPolicy::default()
            }),
        Err(_) => SessionPolicy::default(),
    }
}

pub async fn save(data_dir: &Path, policy: SessionPolicy) -> anyhow::Result<SessionPolicy> {
    let policy = policy.sanitized();
    let path = data_dir.join(FILE_PATH);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, serde_json::to_vec_pretty(&policy)?).await?;
    tokio::fs::rename(&tmp, &path).await?;
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_previous_hardcoded_behaviour() {
        let p = SessionPolicy::default();
        assert_eq!(p.idle_timeout_secs, 86_400);
        assert!(p.reauth_for_funds);
    }

    #[test]
    fn expiry_cannot_be_disabled_by_hand_editing_the_file() {
        let p = SessionPolicy {
            idle_timeout_secs: 0,
            absolute_timeout_secs: Some(0),
            reauth_for_funds: false,
        }
        .sanitized();
        assert!(p.idle_timeout_secs >= MIN_IDLE_SECS);
        assert!(p.absolute_timeout_secs.unwrap() >= MIN_ABSOLUTE_SECS);
    }

    #[test]
    fn absolute_cap_is_never_shorter_than_idle() {
        // Otherwise a session dies while actively in use, which the operator
        // experiences as being logged out at random.
        let p = SessionPolicy {
            idle_timeout_secs: 7 * 24 * 3600,
            absolute_timeout_secs: Some(3600),
            reauth_for_funds: true,
        }
        .sanitized();
        assert_eq!(p.absolute_timeout_secs.unwrap(), p.idle_timeout_secs);
    }

    #[test]
    fn a_kiosk_never_expires_from_idleness_but_still_has_a_ceiling() {
        let p = SessionPolicy::default();
        let a_week = 7 * 24 * 3600;
        assert!(!p.is_expired(DeviceClass::Kiosk, 60, a_week));
        assert!(p.is_expired(DeviceClass::Browser, 60, a_week));
        // The absolute cap still applies to the TV.
        assert!(p.is_expired(DeviceClass::Kiosk, 31 * 24 * 3600, 0));
    }

    #[test]
    fn a_polling_dashboard_still_eventually_expires() {
        // idle never grows because the page polls; only the cap saves us.
        let p = SessionPolicy::default();
        assert!(p.is_expired(DeviceClass::Browser, 30 * 24 * 3600, 0));
    }
}
