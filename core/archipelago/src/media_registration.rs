//! Private, durable preparation for an explicitly authorized IndeeHub Cloud selection.
//!
//! This is NOT an RPC authorization boundary or a content-serving registration.
//! Call only on a blocking worker after authenticating the installed app, fetching
//! its intent through a trusted channel, and obtaining the operator's consent for
//! this exact selection and terms. Do not return the receipt to the app until the
//! caller has durably linked its snapshot to authenticated serving/entitlements.
//! Nothing here advertises content, changes a share policy, or creates an identity.
use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const DOMAIN: &str = "archipelago.indeehub.media-registration.v1";
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const MAX_RECORD_BYTES: u64 = 64 * 1024;
const PRIVATE_DIRECTORY: &str = "media-registration";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Intent {
    pub version: u8,
    pub request_id: String,
    pub nonce: String,
    pub app_audience: String,
    pub node_did: String,
    pub producer: String,
    pub project_id: String,
    pub price_sats: u64,
    pub viewing_seconds: u64,
    pub created_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub version: u8,
    pub request_id: String,
    pub nonce: String,
    pub app_audience: String,
    pub node_did: String,
    pub producer: String,
    pub project_id: String,
    pub price_sats: u64,
    pub viewing_seconds: u64,
    pub expires_at: u64,
    pub content_id: String,
    pub sha256: String,
    /// Decimal string, preserving the full uint64 range in JavaScript.
    pub size_bytes: String,
    pub payment_methods: Vec<String>,
    pub issued_at: u64,
    pub signature: String,
}

impl Receipt {
    /// Exactly the ordered UTF-8 JSON array consumed by IndeeHub v1.
    pub fn preimage(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&serde_json::json!([
            DOMAIN,
            self.request_id,
            self.nonce,
            self.app_audience,
            self.node_did,
            self.producer,
            self.project_id,
            self.content_id,
            self.sha256,
            self.size_bytes,
            self.price_sats,
            self.viewing_seconds,
            self.payment_methods,
            self.issued_at,
            self.expires_at
        ]))?)
    }
}

/// Must come from authenticated installation state, never the request body.
#[derive(Clone, Debug)]
pub struct InstallationPin {
    pub node_did: String,
    pub app_audience: String,
}

/// The caller must have obtained explicit operator consent for all these fields.
/// This struct carries assertions; constructing it does not authenticate them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedSelection {
    pub relative_path: PathBuf,
    /// Sorted, distinct, and restricted to methods this node can actually receive.
    pub payment_methods: Vec<String>,
}

pub struct Limits<'a> {
    pub max_bytes: u64,
    pub cancelled: &'a AtomicBool,
}

/// Private preparation only. Receipt delivery requires caller-owned durable
/// serving/entitlement linkage, described in this module's contract.
#[derive(Debug)]
pub struct PreparedRegistration {
    pub receipt: Receipt,
    pub snapshot_path: PathBuf,
    /// Read-only descriptor for the verified immutable snapshot.
    pub snapshot: File,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceStamp {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
}
impl SourceStamp {
    pub(crate) fn read(file: &File) -> Result<Self> {
        let m = file.metadata()?;
        anyhow::ensure!(
            m.is_file(),
            "The selected Cloud item must be a regular file"
        );
        Ok(Self {
            device: m.dev(),
            inode: m.ino(),
            size: m.len(),
            modified_seconds: m.mtime(),
            modified_nanos: m.mtime_nsec(),
            changed_seconds: m.ctime(),
            changed_nanos: m.ctime_nsec(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    intent: Intent,
    selection: AuthorizedSelection,
    cloud_root: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    version: u8,
    binding: Binding,
    source: SourceStamp,
    issued_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotRecord {
    sha256: String,
    size: u64,
}

fn ascii_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || v == b'_' || v == b'-')
}
fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn validate_intent(
    intent: &Intent,
    pin: &InstallationPin,
    identity: &crate::identity::NodeIdentity,
    now: u64,
) -> Result<()> {
    let id =
        uuid::Uuid::parse_str(&intent.request_id).context("Invalid registration request ID")?;
    anyhow::ensure!(
        id.get_version_num() == 4
            && id.get_variant() == uuid::Variant::RFC4122
            && id.to_string() == intent.request_id,
        "Registration request ID must be a canonical v4 UUID"
    );
    anyhow::ensure!(
        intent.version == 1
            && lower_hex(&intent.nonce, 64)
            && lower_hex(&intent.producer, 64)
            && ascii_id(&intent.project_id)
            && ascii_id(&pin.app_audience)
            && intent.app_audience == pin.app_audience
            && intent.node_did == pin.node_did
            && pin.node_did == identity.did_key()?,
        "Registration identity or installation binding mismatch"
    );
    anyhow::ensure!(
        intent.price_sats <= MAX_SAFE_INTEGER
            && (1..=31_536_000).contains(&intent.viewing_seconds)
            && intent.created_at <= MAX_SAFE_INTEGER
            && intent.expires_at <= MAX_SAFE_INTEGER
            && now <= MAX_SAFE_INTEGER
            && intent.created_at <= now.saturating_add(30)
            && intent.expires_at > intent.created_at
            && intent.expires_at - intent.created_at <= 600,
        "Invalid registration terms or timestamps"
    );
    Ok(())
}
fn validate(
    intent: &Intent,
    selection: &AuthorizedSelection,
    pin: &InstallationPin,
    identity: &crate::identity::NodeIdentity,
    now: u64,
) -> Result<()> {
    validate_intent(intent, pin, identity, now)?;
    anyhow::ensure!(
        !selection.relative_path.as_os_str().is_empty()
            && selection
                .relative_path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && selection.relative_path.as_os_str().as_bytes().len() <= 4096,
        "Cloud selection must be a relative path without traversal"
    );
    let methods = &selection.payment_methods;
    anyhow::ensure!(
        !methods.is_empty()
            && methods.len() <= 4
            && methods.iter().all(|m| matches!(
                m.as_str(),
                "cashu" | "fedimint" | "lightning" | "lightning-cashu"
            ))
            && methods.windows(2).all(|m| m[0] < m[1]),
        "Invalid receiving methods"
    );
    Ok(())
}

fn c_path(path: &Path) -> Result<CString> {
    CString::new(path.as_os_str().as_bytes()).context("Invalid file path")
}
fn fd_result(fd: libc::c_int) -> Result<File> {
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: the successful syscall returned a newly owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}
pub(crate) fn open_at(
    dir: &File,
    name: &str,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> Result<File> {
    let name = CString::new(name)?;
    // SAFETY: descriptor and NUL-terminated name remain alive through the call.
    fd_result(unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    })
}
pub(crate) fn open_directory(path: &Path) -> Result<File> {
    let path = c_path(path)?;
    // SAFETY: path is a valid NUL-terminated string.
    fd_result(unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    })
}
pub(crate) fn private_directory(parent: &File, name: &str) -> Result<File> {
    let c_name = CString::new(name)?;
    // SAFETY: valid directory descriptor and string; no existing data is replaced.
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), c_name.as_ptr(), 0o700) };
    if result < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error.into());
        }
    }
    let dir = open_at(parent, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let metadata = dir.metadata()?;
    // SAFETY: geteuid has no pointer arguments or side effects.
    anyhow::ensure!(
        metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o077 == 0,
        "Registration storage must be private to the node service"
    );
    parent.sync_all()?;
    Ok(dir)
}

#[cfg(target_os = "linux")]
pub(crate) fn open_cloud_file(root: &File, relative: &Path) -> Result<File> {
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let name = c_path(relative)?;
    let how = OpenHow {
        flags: (libc::O_PATH | libc::O_CLOEXEC | libc::O_NOFOLLOW) as u64,
        mode: 0,
        resolve: 0x08 | 0x04,
    }; // RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS
       // SAFETY: openat2 receives a held directory descriptor, valid name and a
       // correctly sized immutable open_how. Fail closed on unsupported kernels.
    let path_fd = fd_result(unsafe {
        libc::syscall(
            libc::SYS_openat2,
            root.as_raw_fd(),
            name.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        ) as libc::c_int
    })
    .context("Could not safely open the Cloud selection (Linux openat2 required)")?;
    let expected = SourceStamp::read(&path_fd)?;
    // Reopen only our already-held regular-file descriptor, not its mutable path.
    // O_PATH lets us reject devices/FIFOs before invoking their open operations.
    let descriptor = CString::new(format!("/proc/self/fd/{}", path_fd.as_raw_fd()))?;
    let file = fd_result(unsafe {
        libc::open(
            descriptor.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    })?;
    anyhow::ensure!(
        SourceStamp::read(&file)? == expected,
        "Cloud selection changed while opening"
    );
    Ok(file)
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn open_cloud_file(_root: &File, _relative: &Path) -> Result<File> {
    anyhow::bail!("Safe Cloud registration currently requires Linux openat2")
}

fn cancelled(limits: &Limits<'_>) -> Result<()> {
    anyhow::ensure!(
        !limits.cancelled.load(Ordering::Relaxed),
        "Media registration cancelled"
    );
    Ok(())
}
pub(crate) fn lock_operation(dir: &File, limits: &Limits<'_>, deadline: Instant) -> Result<()> {
    loop {
        cancelled(limits)?;
        anyhow::ensure!(
            Instant::now() < deadline,
            "Timed out waiting for this registration operation"
        );
        // SAFETY: flock borrows the directory descriptor; closing it releases lock.
        if unsafe { libc::flock(dir.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::WouldBlock
            && error.kind() != std::io::ErrorKind::Interrupted
        {
            return Err(error.into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
pub(crate) fn read_record<T: serde::de::DeserializeOwned>(
    dir: &File,
    name: &str,
) -> Result<Option<T>> {
    let file = match open_at(dir, name, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(file) => file,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(None)
        }
        Err(error) => return Err(error),
    };
    let m = file.metadata()?;
    anyhow::ensure!(
        m.is_file() && m.len() <= MAX_RECORD_BYTES && m.mode() & 0o077 == 0,
        "Invalid private registration record"
    );
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_RECORD_BYTES,
        "Registration record is too large"
    );
    Ok(Some(serde_json::from_slice(&bytes).context(
        "Damaged registration record; preserve it for recovery",
    )?))
}
pub(crate) fn temporary(dir: &File) -> Result<(String, File)> {
    let name = format!("pending-{}", uuid::Uuid::new_v4());
    Ok((
        name.clone(),
        open_at(
            dir,
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?,
    ))
}
pub(crate) fn publish_file(dir: &File, temporary: &str, final_name: &str) -> Result<()> {
    let from = CString::new(temporary)?;
    let to = CString::new(final_name)?;
    // linkat publishes without replacing an existing destination. Both names
    // are generated locally inside the locked private operation directory.
    if unsafe {
        libc::linkat(
            dir.as_raw_fd(),
            from.as_ptr(),
            dir.as_raw_fd(),
            to.as_ptr(),
            0,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    dir.sync_all()?;
    if unsafe { libc::unlinkat(dir.as_raw_fd(), from.as_ptr(), 0) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    dir.sync_all()?;
    Ok(())
}
pub(crate) fn save_record<T: Serialize>(dir: &File, name: &str, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_RECORD_BYTES,
        "Registration record is too large"
    );
    let (temporary, mut file) = temporary(dir)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    publish_file(dir, &temporary, name)
}

pub(crate) fn hash_file(
    file: &mut File,
    mut output: Option<&mut File>,
    limits: &Limits<'_>,
    progress: &mut impl FnMut(u64) -> Result<()>,
) -> Result<(String, u64)> {
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        cancelled(limits)?;
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        size = size
            .checked_add(count as u64)
            .context("Media size overflow")?;
        anyhow::ensure!(
            size <= limits.max_bytes,
            "Media exceeds the approved snapshot size limit"
        );
        hash.update(&chunk[..count]);
        if let Some(destination) = output.as_mut() {
            destination.write_all(&chunk[..count])?;
        }
        progress(size)?;
    }
    cancelled(limits)?;
    Ok((hex::encode(hash.finalize()), size))
}
fn receipt_for(operation: &Operation, hash: String, size: u64) -> Receipt {
    let intent = &operation.binding.intent;
    Receipt {
        version: 1,
        request_id: intent.request_id.clone(),
        nonce: intent.nonce.clone(),
        app_audience: intent.app_audience.clone(),
        node_did: intent.node_did.clone(),
        producer: intent.producer.clone(),
        project_id: intent.project_id.clone(),
        price_sats: intent.price_sats,
        viewing_seconds: intent.viewing_seconds,
        expires_at: intent.expires_at,
        content_id: format!("registered_{}", intent.request_id),
        sha256: hash,
        size_bytes: size.to_string(),
        payment_methods: operation.binding.selection.payment_methods.clone(),
        issued_at: operation.issued_at,
        signature: String::new(),
    }
}

const RETIREMENT_DOMAIN: &str = "archipelago.indeehub.media-retirement.v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Retirement {
    pub version: u8,
    pub intent: Intent,
    pub retired_at: u64,
    pub signature: String,
}
impl Retirement {
    pub fn preimage(&self) -> Result<Vec<u8>> {
        let i = &self.intent;
        Ok(serde_json::to_vec(&serde_json::json!([
            RETIREMENT_DOMAIN,
            i.request_id,
            i.nonce,
            i.app_audience,
            i.node_did,
            i.producer,
            i.project_id,
            i.price_sats,
            i.viewing_seconds,
            i.created_at,
            i.expires_at,
            self.retired_at
        ]))?)
    }
    fn verify(&self, intent: &Intent, identity: &crate::identity::NodeIdentity) -> Result<()> {
        anyhow::ensure!(
            self.version == 1
                && self.intent == *intent
                && self.retired_at >= intent.expires_at
                && self.retired_at <= MAX_SAFE_INTEGER
                && lower_hex(&self.signature, 128),
            "Registration retirement terms changed"
        );
        identity.signing_key().verifying_key().verify_strict(
            &self.preimage()?,
            &Signature::from_slice(&hex::decode(&self.signature)?)?,
        )?;
        Ok(())
    }
}
pub enum Resolution {
    Prepared {
        prepared: PreparedRegistration,
        selection: AuthorizedSelection,
    },
    Retired(Retirement),
    Pending {
        request_id: String,
        expires_at: u64,
    },
}
/// Caller authenticates the producer's intent-only recovery/retirement consent.
/// Under the original operation lock, either verify the completed bytes/receipt,
/// or commit retirement. No source path from this request is opened or copied.
pub fn resolve(
    data_dir: &Path,
    identity: &crate::identity::NodeIdentity,
    pin: &InstallationPin,
    intent: &Intent,
    now: u64,
    limits: &Limits<'_>,
) -> Result<Resolution> {
    validate_intent(intent, pin, identity, now)?;
    let data_dir = data_dir.canonicalize()?;
    let data = open_directory(&data_dir)?;
    let root = private_directory(&data, PRIVATE_DIRECTORY)?;
    let dir = private_directory(&root, &intent.request_id)?;
    lock_operation(&dir, limits, Instant::now() + Duration::from_secs(30))?;
    if let Some(retired) = read_record::<Retirement>(&dir, "retirement.json")? {
        retired.verify(intent, identity)?;
        dir.sync_all()?;
        if let Some(operation) = read_record::<Operation>(&dir, "operation.json")? {
            anyhow::ensure!(
                operation.version == 1 && operation.binding.intent == *intent,
                "Retired operation terms changed"
            );
            crate::snapshot_budget::finish_completed(
                &data_dir,
                &format!("registered:{}", intent.request_id),
                operation.source.size,
                limits,
            )?;
        }
        return Ok(Resolution::Retired(retired));
    }
    let operation: Option<Operation> = read_record(&dir, "operation.json")?;
    if let Some(operation) = &operation {
        anyhow::ensure!(
            operation.version == 1 && operation.binding.intent == *intent,
            "Original registration intent changed"
        );
        validate(intent, &operation.binding.selection, pin, identity, now)?;
    }
    if let Some(receipt) = read_record::<Receipt>(&dir, "receipt.json")? {
        let operation =
            operation.context("Receipt has no original operation; preserve recovery data")?;
        let saved: SnapshotRecord =
            read_record(&dir, "snapshot.json")?.context("Snapshot commitment missing")?;
        let mut snapshot = open_at(&dir, "media", libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
        anyhow::ensure!(
            snapshot.metadata()?.mode() & 0o7777 == 0o400,
            "Snapshot permissions changed"
        );
        let before = SourceStamp::read(&snapshot)?;
        let (sha256, size) = hash_file(&mut snapshot, None, limits, &mut |_| Ok(()))?;
        anyhow::ensure!(
            SourceStamp::read(&snapshot)? == before
                && size == operation.source.size
                && sha256 == saved.sha256
                && size == saved.size,
            "Completed registration bytes changed"
        );
        let mut expected = receipt_for(&operation, sha256, size);
        anyhow::ensure!(
            lower_hex(&receipt.signature, 128),
            "Invalid completed signature"
        );
        identity.signing_key().verifying_key().verify_strict(
            &receipt.preimage()?,
            &Signature::from_slice(&hex::decode(&receipt.signature)?)?,
        )?;
        expected.signature = receipt.signature.clone();
        anyhow::ensure!(
            expected == receipt,
            "Completed registration receipt changed"
        );
        snapshot.seek(SeekFrom::Start(0))?;
        dir.sync_all()?;
        crate::snapshot_budget::finish_completed(
            &data_dir,
            &format!("registered:{}", intent.request_id),
            operation.source.size,
            limits,
        )?;
        return Ok(Resolution::Prepared {
            prepared: PreparedRegistration {
                receipt,
                snapshot,
                snapshot_path: data_dir
                    .join(PRIVATE_DIRECTORY)
                    .join(&intent.request_id)
                    .join("media"),
            },
            selection: operation.binding.selection,
        });
    }
    if now < intent.expires_at {
        return Ok(Resolution::Pending {
            request_id: intent.request_id.clone(),
            expires_at: intent.expires_at,
        });
    }
    // Missing operation metadata alongside media is damaged state, not proof
    // that an earlier completion never happened. Preserve it for investigation.
    if operation.is_none() {
        anyhow::ensure!(
            read_record::<SnapshotRecord>(&dir, "snapshot.json")?.is_none(),
            "Snapshot exists without original intent; preserve recovery data"
        );
        match open_at(&dir, "media", libc::O_RDONLY | libc::O_NONBLOCK, 0) {
            Ok(_) => anyhow::bail!("Media exists without original intent; preserve recovery data"),
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    let mut retirement = Retirement {
        version: 1,
        intent: intent.clone(),
        retired_at: now,
        signature: String::new(),
    };
    retirement.signature = hex::encode(
        identity
            .signing_key()
            .sign(&retirement.preimage()?)
            .to_bytes(),
    );
    save_record(&dir, "retirement.json", &retirement)?;
    if let Some(operation) = operation {
        // Tombstone is durable under the copy lock: no writer can resume. Any
        // retained partial bytes stay counted; no media or source is deleted.
        crate::snapshot_budget::finish_completed(
            &data_dir,
            &format!("registered:{}", intent.request_id),
            operation.source.size,
            limits,
        )?;
    }
    Ok(Resolution::Retired(retirement))
}

/// Performs disk I/O and cross-process locking; run on a blocking worker.
/// `now` is caller-supplied trusted UTC seconds, never a request-body timestamp.
/// `progress` may return an error to cancel; it must not change authorized terms.
/// A completed exact retry may return the original receipt after expiry. An
/// incomplete operation cannot create a new receipt after its intent expires.
/// Failed/cancelled staging files are retained privately, never publicly served.
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    data_dir: &Path,
    cloud_root: &Path,
    identity: &crate::identity::NodeIdentity,
    pin: &InstallationPin,
    intent: &Intent,
    selection: &AuthorizedSelection,
    now: u64,
    limits: &Limits<'_>,
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<PreparedRegistration> {
    let started = Instant::now();
    validate(intent, selection, pin, identity, now)?;
    cancelled(limits)?;
    let data_dir = data_dir
        .canonicalize()
        .context("Node data directory unavailable")?;
    let cloud_root = cloud_root
        .canonicalize()
        .context("Configured Cloud root unavailable")?;
    let binding = Binding {
        intent: intent.clone(),
        selection: selection.clone(),
        cloud_root: cloud_root.clone(),
    };
    let data = open_directory(&data_dir)?;
    let root = private_directory(&data, PRIVATE_DIRECTORY)?;
    let operation_dir = private_directory(&root, &intent.request_id)?;
    // Never wait indefinitely for another worker/process. Live intents also stop
    // waiting at their expiry; a completed expired replay gets a bounded 30s wait.
    let wait_seconds = if now < intent.expires_at {
        (intent.expires_at - now).min(30)
    } else {
        30
    };
    lock_operation(
        &operation_dir,
        limits,
        started + Duration::from_secs(wait_seconds),
    )?;
    if let Some(retired) = read_record::<Retirement>(&operation_dir, "retirement.json")? {
        retired.verify(intent, identity)?;
        anyhow::bail!("Registration was authoritatively retired; recover that result before starting a new intent");
    }
    let operation: Operation =
        if let Some(saved) = read_record::<Operation>(&operation_dir, "operation.json")? {
            anyhow::ensure!(
                saved.version == 1
                    && saved.binding == binding
                    && saved.issued_at >= intent.created_at.saturating_sub(30)
                    && saved.issued_at < intent.expires_at,
                "Registration request was already bound to different terms"
            );
            saved
        } else {
            anyhow::ensure!(now < intent.expires_at, "Registration intent expired");
            let cloud = open_directory(&cloud_root)?;
            let source = open_cloud_file(&cloud, &selection.relative_path)?;
            let stamp = SourceStamp::read(&source)?;
            anyhow::ensure!(
                stamp.size <= limits.max_bytes,
                "Media exceeds the approved snapshot size limit"
            );
            let operation = Operation {
                version: 1,
                binding,
                source: stamp,
                issued_at: now,
            };
            save_record(&operation_dir, "operation.json", &operation)?;
            operation
        };
    let existing_receipt: Option<Receipt> = read_record(&operation_dir, "receipt.json")?;
    let mut saved_snapshot: Option<SnapshotRecord> = read_record(&operation_dir, "snapshot.json")?;
    let mut copied = None;
    let mut snapshot = match open_at(
        &operation_dir,
        "media",
        libc::O_RDONLY | libc::O_NONBLOCK,
        0,
    ) {
        Ok(file) => file,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            anyhow::ensure!(
                existing_receipt.is_none(),
                "Registered snapshot is missing; preserve the receipt for recovery"
            );
            anyhow::ensure!(
                now.saturating_add(started.elapsed().as_secs()) < intent.expires_at,
                "Registration intent expired"
            );
            let cloud = open_directory(&cloud_root)?;
            let mut source = open_cloud_file(&cloud, &selection.relative_path)?;
            anyhow::ensure!(
                SourceStamp::read(&source)? == operation.source,
                "Selected Cloud file changed; use a new reviewed intent"
            );
            let _reservation = crate::snapshot_budget::reserve_until(
                &data_dir,
                &format!("registered:{}", intent.request_id),
                operation.source.size,
                crate::snapshot_budget::DEFAULT_MAX_TOTAL_BYTES,
                crate::snapshot_budget::DEFAULT_MIN_FREE_BYTES,
                limits,
                started + Duration::from_secs(intent.expires_at.saturating_sub(now).min(30)),
            )?;
            anyhow::ensure!(
                now.saturating_add(started.elapsed().as_secs()) < intent.expires_at,
                "Registration expired while awaiting snapshot capacity"
            );
            let (name, mut destination) = temporary(&operation_dir)?;
            let (hash, size) =
                hash_file(&mut source, Some(&mut destination), limits, &mut progress)?;
            anyhow::ensure!(
                SourceStamp::read(&source)? == operation.source && size == operation.source.size,
                "Selected Cloud file changed while copying; no receipt issued"
            );
            destination.set_permissions(std::fs::Permissions::from_mode(0o400))?;
            destination.sync_all()?;
            cancelled(limits)?;
            let record = SnapshotRecord {
                sha256: hash.clone(),
                size,
            };
            if let Some(saved) = &saved_snapshot {
                anyhow::ensure!(
                    *saved == record,
                    "Snapshot recovery produced different bytes"
                );
            } else {
                save_record(&operation_dir, "snapshot.json", &record)?;
                saved_snapshot = Some(record);
            }
            publish_file(&operation_dir, &name, "media")?;
            copied = Some((hash, size));
            open_at(
                &operation_dir,
                "media",
                libc::O_RDONLY | libc::O_NONBLOCK,
                0,
            )?
        }
        Err(error) => return Err(error),
    };
    let before = SourceStamp::read(&snapshot)?;
    anyhow::ensure!(
        snapshot.metadata()?.mode() & 0o7777 == 0o400,
        "Registered snapshot permissions changed"
    );
    let (hash, size) = match copied {
        Some(result) => result,
        None => hash_file(&mut snapshot, None, limits, &mut progress)?,
    };
    anyhow::ensure!(
        SourceStamp::read(&snapshot)? == before && size == operation.source.size,
        "Registered snapshot changed or is incomplete"
    );
    let saved_snapshot = saved_snapshot
        .context("Snapshot commitment is missing; preserve registration data for recovery")?;
    anyhow::ensure!(
        saved_snapshot.sha256 == hash && saved_snapshot.size == size,
        "Registered snapshot does not match its durable byte commitment"
    );
    let mut receipt = receipt_for(&operation, hash, size);
    if let Some(saved) = existing_receipt {
        anyhow::ensure!(
            lower_hex(&saved.signature, 128),
            "Invalid stored registration signature encoding"
        );
        let signature = Signature::from_slice(&hex::decode(&saved.signature)?)?;
        identity
            .signing_key()
            .verifying_key()
            .verify_strict(&saved.preimage()?, &signature)?;
        receipt.signature = saved.signature.clone();
        anyhow::ensure!(
            receipt == saved,
            "Stored registration receipt does not match its immutable snapshot"
        );
    } else {
        anyhow::ensure!(
            now.saturating_add(started.elapsed().as_secs()) < intent.expires_at,
            "Registration intent expired before completion; no receipt issued"
        );
        receipt.signature =
            hex::encode(identity.signing_key().sign(&receipt.preimage()?).to_bytes());
        save_record(&operation_dir, "receipt.json", &receipt)?;
    }
    operation_dir.sync_all()?;
    crate::snapshot_budget::finish_completed(
        &data_dir,
        &format!("registered:{}", intent.request_id),
        operation.source.size,
        limits,
    )?;
    // Reopen from the held directory to return a descriptor positioned at zero.
    let snapshot = open_at(
        &operation_dir,
        "media",
        libc::O_RDONLY | libc::O_NONBLOCK,
        0,
    )?;
    Ok(PreparedRegistration {
        receipt,
        snapshot,
        snapshot_path: data_dir
            .join(PRIVATE_DIRECTORY)
            .join(&intent.request_id)
            .join("media"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Fixture {
        root: tempfile::TempDir,
        identity: crate::identity::NodeIdentity,
        pin: InstallationPin,
        intent: Intent,
        selection: AuthorizedSelection,
        cancelled: AtomicBool,
    }
    impl Fixture {
        async fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join("cloud")).unwrap();
            std::fs::write(root.path().join("cloud/film.mp4"), vec![42u8; 150_000]).unwrap();
            let identity =
                crate::identity::NodeIdentity::load_or_create(&root.path().join("identity"))
                    .await
                    .unwrap();
            let pin = InstallationPin {
                node_did: identity.did_key().unwrap(),
                app_audience: "fixture-indeehub".into(),
            };
            let intent = Intent {
                version: 1,
                request_id: uuid::Uuid::new_v4().to_string(),
                nonce: "ab".repeat(32),
                app_audience: pin.app_audience.clone(),
                node_did: pin.node_did.clone(),
                producer: "cd".repeat(32),
                project_id: "fixture-project".into(),
                price_sats: 15,
                viewing_seconds: 3600,
                created_at: 1000,
                expires_at: 1600,
            };
            Self {
                root,
                identity,
                pin,
                intent,
                selection: AuthorizedSelection {
                    relative_path: "film.mp4".into(),
                    payment_methods: vec!["cashu".into(), "lightning-cashu".into()],
                },
                cancelled: AtomicBool::new(false),
            }
        }
        fn run(&self, now: u64) -> Result<PreparedRegistration> {
            self.with_progress(now, |_| Ok(()))
        }
        fn with_progress(
            &self,
            now: u64,
            progress: impl FnMut(u64) -> Result<()>,
        ) -> Result<PreparedRegistration> {
            prepare(
                self.root.path(),
                &self.root.path().join("cloud"),
                &self.identity,
                &self.pin,
                &self.intent,
                &self.selection,
                now,
                &Limits {
                    max_bytes: 1_000_000,
                    cancelled: &self.cancelled,
                },
                progress,
            )
        }
        fn operation_dir(&self) -> PathBuf {
            self.root
                .path()
                .join(PRIVATE_DIRECTORY)
                .join(&self.intent.request_id)
        }
    }

    #[tokio::test]
    async fn independent_nodejs_golden_wire_and_signature_match_prepared_receipt() {
        let vector: serde_json::Value =
            serde_json::from_str(include_str!("media_registration/fixtures/v1.json")).unwrap();
        let mut fixture = Fixture::new().await;
        // Fixed public test seed only, loaded through the existing identity API.
        std::fs::write(
            fixture.root.path().join("identity/node_key"),
            hex::decode(vector["testSeedHex"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        fixture.identity =
            crate::identity::NodeIdentity::load_existing(&fixture.root.path().join("identity"))
                .await
                .unwrap();
        fixture.pin = InstallationPin {
            node_did: vector["pin"]["nodeDid"].as_str().unwrap().into(),
            app_audience: vector["pin"]["appAudience"].as_str().unwrap().into(),
        };
        fixture.intent = serde_json::from_value(vector["intent"].clone()).unwrap();
        std::fs::write(
            fixture.root.path().join("cloud/film.mp4"),
            vector["mediaUtf8"].as_str().unwrap(),
        )
        .unwrap();
        let expected: Receipt = serde_json::from_value(vector["receipt"].clone()).unwrap();
        assert_eq!(
            fixture.identity.pubkey_hex(),
            vector["pin"]["publicKey"].as_str().unwrap()
        );
        assert_eq!(
            expected.preimage().unwrap(),
            vector["preimageUtf8"].as_str().unwrap().as_bytes()
        );
        assert!(crate::identity::NodeIdentity::verify(
            &fixture.identity.pubkey_hex(),
            vector["preimageUtf8"].as_str().unwrap().as_bytes(),
            &expected.signature
        )
        .unwrap());
        assert_eq!(fixture.run(1000).unwrap().receipt, expected);
    }

    #[tokio::test]
    async fn independent_nodejs_retirement_wire_matches_terminal_record() {
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "media_registration/fixtures/retirement-v1.json"
        ))
        .unwrap();
        let mut fixture = Fixture::new().await;
        std::fs::write(
            fixture.root.path().join("identity/node_key"),
            hex::decode(vector["testSeedHex"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        fixture.identity =
            crate::identity::NodeIdentity::load_existing(&fixture.root.path().join("identity"))
                .await
                .unwrap();
        fixture.pin = InstallationPin {
            node_did: vector["pin"]["nodeDid"].as_str().unwrap().into(),
            app_audience: vector["pin"]["appAudience"].as_str().unwrap().into(),
        };
        fixture.intent = serde_json::from_value(vector["intent"].clone()).unwrap();
        let expected: Retirement = serde_json::from_value(vector["retirement"].clone()).unwrap();
        assert_eq!(
            expected.preimage().unwrap(),
            vector["preimageUtf8"].as_str().unwrap().as_bytes()
        );
        expected.verify(&fixture.intent, &fixture.identity).unwrap();
        let result = resolve(
            fixture.root.path(),
            &fixture.identity,
            &fixture.pin,
            &fixture.intent,
            expected.retired_at,
            &Limits {
                max_bytes: 1024,
                cancelled: &fixture.cancelled,
            },
        )
        .unwrap();
        match result {
            Resolution::Retired(actual) => assert_eq!(actual, expected),
            _ => panic!("expected retirement"),
        }
    }

    #[test]
    fn expired_lock_deadline_returns_without_waiting() {
        let root = tempfile::tempdir().unwrap();
        let dir = open_directory(root.path()).unwrap();
        let cancelled = AtomicBool::new(false);
        assert!(lock_operation(
            &dir,
            &Limits {
                max_bytes: 1,
                cancelled: &cancelled
            },
            Instant::now()
        )
        .is_err());
        cancelled.store(true, Ordering::Relaxed);
        assert!(lock_operation(
            &dir,
            &Limits {
                max_bytes: 1,
                cancelled: &cancelled
            },
            Instant::now() + Duration::from_secs(30)
        )
        .is_err());
    }

    #[tokio::test]
    async fn snapshot_and_signed_receipt_are_durable_private_and_exactly_recoverable() {
        let fixture = Fixture::new().await;
        let key_before = std::fs::read(fixture.root.path().join("identity/node_key")).unwrap();
        let mut prepared = fixture.run(1000).unwrap();
        let mut bytes = Vec::new();
        prepared.snapshot.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, vec![42u8; 150_000]);
        assert_eq!(prepared.receipt.sha256, hex::encode(Sha256::digest(&bytes)));
        assert_eq!(prepared.receipt.size_bytes, "150000");
        assert_eq!(prepared.snapshot.metadata().unwrap().mode() & 0o7777, 0o400);
        assert_eq!(
            fixture.operation_dir().metadata().unwrap().mode() & 0o777,
            0o700
        );
        assert!(crate::identity::NodeIdentity::verify(
            &fixture.identity.pubkey_hex(),
            &prepared.receipt.preimage().unwrap(),
            &prepared.receipt.signature
        )
        .unwrap());
        let wire = serde_json::to_value(&prepared.receipt).unwrap();
        assert_eq!(wire.as_object().unwrap().len(), 16);
        assert_eq!(wire["appAudience"], "fixture-indeehub");
        assert!(wire.get("createdAt").is_none());
        let expected = serde_json::json!([
            DOMAIN,
            fixture.intent.request_id,
            fixture.intent.nonce,
            fixture.pin.app_audience,
            fixture.pin.node_did,
            fixture.intent.producer,
            fixture.intent.project_id,
            prepared.receipt.content_id,
            prepared.receipt.sha256,
            "150000",
            15,
            3600,
            ["cashu", "lightning-cashu"],
            1000,
            1600
        ]);
        assert_eq!(
            prepared.receipt.preimage().unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        // A finished retry uses the retained bytes, even after source removal and expiry.
        std::fs::remove_file(fixture.root.path().join("cloud/film.mp4")).unwrap();
        assert_eq!(fixture.run(1700).unwrap().receipt, prepared.receipt);
        assert_eq!(
            std::fs::read(fixture.root.path().join("identity/node_key")).unwrap(),
            key_before
        );
        assert!(!fixture.root.path().join("content.json").exists());
    }

    #[tokio::test]
    async fn changed_terms_identity_selection_or_methods_cannot_reuse_a_request() {
        let mut fixture = Fixture::new().await;
        let original = fixture.run(1000).unwrap().receipt;
        fixture.intent.price_sats += 1;
        assert!(fixture.run(1000).is_err());
        fixture.intent.price_sats -= 1;
        fixture.intent.nonce = "ef".repeat(32);
        assert!(fixture.run(1000).is_err());
        fixture.intent.nonce = "ab".repeat(32);
        fixture.selection.relative_path = "other.mp4".into();
        assert!(fixture.run(1000).is_err());
        fixture.selection.relative_path = "film.mp4".into();
        fixture.selection.payment_methods = vec!["lightning".into()];
        assert!(fixture.run(1000).is_err());
        fixture.selection.payment_methods = vec!["cashu".into(), "lightning-cashu".into()];
        fixture.pin.app_audience = "different-installation".into();
        assert!(fixture.run(1000).is_err());
        fixture.pin.app_audience = "fixture-indeehub".into();
        assert_eq!(fixture.run(1000).unwrap().receipt, original);
    }

    #[tokio::test]
    async fn traversal_symlinks_directories_and_special_files_are_rejected() {
        let mut fixture = Fixture::new().await;
        let outside = fixture.root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret"), b"not selected").unwrap();
        std::os::unix::fs::symlink(&outside, fixture.root.path().join("cloud/escape")).unwrap();
        std::os::unix::fs::symlink("film.mp4", fixture.root.path().join("cloud/link")).unwrap();
        let fifo = c_path(&fixture.root.path().join("cloud/fifo")).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        for path in [
            "../outside/secret",
            "/etc/passwd",
            "escape/secret",
            "link",
            ".",
            "fifo",
        ] {
            fixture.intent.request_id = uuid::Uuid::new_v4().to_string();
            fixture.selection.relative_path = path.into();
            assert!(fixture.run(1000).is_err(), "accepted {path}");
            assert!(!fixture.operation_dir().join("receipt.json").exists());
        }
        assert_eq!(
            std::fs::read(outside.join("secret")).unwrap(),
            b"not selected"
        );
    }

    #[tokio::test]
    async fn cancellation_preserves_bound_operation_and_retries_without_changing_receipt_terms() {
        let fixture = Fixture::new().await;
        assert!(fixture
            .with_progress(1000, |_| {
                fixture.cancelled.store(true, Ordering::Relaxed);
                Ok(())
            })
            .is_err());
        assert!(fixture.operation_dir().join("operation.json").exists());
        assert!(!fixture.operation_dir().join("receipt.json").exists());
        fixture.cancelled.store(false, Ordering::Relaxed);
        let receipt = fixture.run(1001).unwrap().receipt;
        assert_eq!(receipt.issued_at, 1000);
        assert_eq!(fixture.run(1002).unwrap().receipt, receipt);
    }

    #[tokio::test]
    async fn changed_cloud_bytes_during_copy_or_before_retry_never_get_a_receipt() {
        let fixture = Fixture::new().await;
        let path = fixture.root.path().join("cloud/film.mp4");
        let mut changed = false;
        assert!(fixture
            .with_progress(1000, |_| {
                if !changed {
                    File::options().write(true).open(&path)?.set_len(10)?;
                    changed = true;
                }
                Ok(())
            })
            .is_err());
        assert!(!fixture.operation_dir().join("receipt.json").exists());
        assert!(fixture.run(1001).is_err());
        assert_eq!(path.metadata().unwrap().len(), 10);
    }

    #[tokio::test]
    async fn parallel_callers_receive_one_persisted_receipt() {
        let fixture = Arc::new(Fixture::new().await);
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let fixture = Arc::clone(&fixture);
                std::thread::spawn(move || fixture.run(1000).unwrap().receipt)
            })
            .collect();
        let receipts: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert!(receipts.iter().all(|receipt| receipt == &receipts[0]));
        assert_eq!(fixture.run(1000).unwrap().receipt, receipts[0]);
    }

    #[tokio::test]
    async fn durable_snapshot_recovers_before_receipt_but_expired_pending_work_cannot_sign() {
        let fixture = Fixture::new().await;
        let receipt = fixture.run(1000).unwrap().receipt;
        // Simulate interruption after snapshot commit and before receipt persistence.
        std::fs::remove_file(fixture.operation_dir().join("receipt.json")).unwrap();
        std::fs::remove_file(fixture.root.path().join("cloud/film.mp4")).unwrap();
        assert!(fixture.run(1600).is_err());
        assert_eq!(fixture.run(1001).unwrap().receipt, receipt);
    }

    #[tokio::test]
    async fn corrupt_snapshot_or_missing_commitment_never_reissues_a_receipt() {
        let fixture = Fixture::new().await;
        let prepared = fixture.run(1000).unwrap();
        std::fs::set_permissions(
            &prepared.snapshot_path,
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::fs::write(&prepared.snapshot_path, vec![21u8; 150_000]).unwrap();
        std::fs::set_permissions(
            &prepared.snapshot_path,
            std::fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        assert!(fixture.run(1000).is_err());
        std::fs::remove_file(fixture.operation_dir().join("receipt.json")).unwrap();
        assert!(fixture.run(1000).is_err());
        std::fs::remove_file(fixture.operation_dir().join("snapshot.json")).unwrap();
        assert!(fixture.run(1000).is_err());
    }

    #[tokio::test]
    async fn invalid_intents_quota_and_damaged_records_fail_without_replacing_data() {
        let mut fixture = Fixture::new().await;
        fixture.intent.price_sats = MAX_SAFE_INTEGER + 1;
        assert!(fixture.run(1000).is_err());
        fixture.intent.price_sats = 15;
        assert!(fixture.run(1600).is_err());
        assert!(prepare(
            fixture.root.path(),
            &fixture.root.path().join("cloud"),
            &fixture.identity,
            &fixture.pin,
            &fixture.intent,
            &fixture.selection,
            1000,
            &Limits {
                max_bytes: 1,
                cancelled: &fixture.cancelled
            },
            |_| Ok(())
        )
        .is_err());
        assert!(!fixture.operation_dir().join("operation.json").exists());
        std::fs::write(
            fixture.operation_dir().join("operation.json"),
            b"damaged fixture",
        )
        .unwrap();
        assert!(fixture.run(1000).is_err());
        assert_eq!(
            std::fs::read(fixture.operation_dir().join("operation.json")).unwrap(),
            b"damaged fixture"
        );
    }
    #[tokio::test]
    async fn completed_registration_releases_crashed_reservation_without_source_or_new_admission() {
        let fixture = Fixture::new().await;
        let original = fixture.run(1100).unwrap();
        let operation = format!("registered:{}", fixture.intent.request_id);
        let limits = Limits {
            max_bytes: 1_000_000,
            cancelled: &fixture.cancelled,
        };
        let reservation = crate::snapshot_budget::reserve(
            fixture.root.path(),
            &operation,
            150_000,
            4 * 1024 * 1024,
            0,
            &limits,
        )
        .unwrap();
        drop(reservation); // Crash after durable receipt, before reservation cleanup.
        let name = format!("{}.json", hex::encode(Sha256::digest(operation.as_bytes())));
        let ledger = fixture.root.path().join("snapshot-reservations").join(name);
        assert!(ledger.exists());
        std::fs::remove_file(fixture.root.path().join("cloud/film.mp4")).unwrap();
        let recovered = fixture.run(1700).unwrap();
        assert_eq!(recovered.receipt, original.receipt);
        assert!(!ledger.exists());
    }
    #[tokio::test]
    async fn retirement_is_durable_exact_and_prevents_clock_rollback_or_late_prepare_resurrection()
    {
        let fixture = Fixture::new().await;
        let limits = Limits {
            max_bytes: 1_000_000,
            cancelled: &fixture.cancelled,
        };
        assert!(matches!(
            resolve(
                fixture.root.path(),
                &fixture.identity,
                &fixture.pin,
                &fixture.intent,
                1100,
                &limits
            )
            .unwrap(),
            Resolution::Pending { .. }
        ));
        let first = match resolve(
            fixture.root.path(),
            &fixture.identity,
            &fixture.pin,
            &fixture.intent,
            1700,
            &limits,
        )
        .unwrap()
        {
            Resolution::Retired(v) => v,
            _ => panic!("expected retirement"),
        };
        first.verify(&fixture.intent, &fixture.identity).unwrap();
        let again = match resolve(
            fixture.root.path(),
            &fixture.identity,
            &fixture.pin,
            &fixture.intent,
            1800,
            &limits,
        )
        .unwrap()
        {
            Resolution::Retired(v) => v,
            _ => panic!("expected original retirement"),
        };
        assert_eq!(first, again);
        assert!(fixture.run(1100).is_err()); // even a later clock rollback cannot reopen it
        assert!(!fixture.operation_dir().join("media").exists());
        let mut changed = fixture.intent.clone();
        changed.price_sats += 1;
        assert!(resolve(
            fixture.root.path(),
            &fixture.identity,
            &fixture.pin,
            &changed,
            1800,
            &limits
        )
        .is_err());
    }
    #[tokio::test]
    async fn completed_resolution_after_expiry_never_retires_or_copies_removed_source() {
        let fixture = Fixture::new().await;
        let original = fixture.run(1100).unwrap();
        std::fs::remove_file(fixture.root.path().join("cloud/film.mp4")).unwrap();
        for now in [1700, 1800] {
            let resolved = resolve(
                fixture.root.path(),
                &fixture.identity,
                &fixture.pin,
                &fixture.intent,
                now,
                &Limits {
                    max_bytes: 1_000_000,
                    cancelled: &fixture.cancelled,
                },
            )
            .unwrap();
            match resolved {
                Resolution::Prepared {
                    prepared,
                    selection,
                } => {
                    assert_eq!(prepared.receipt, original.receipt);
                    assert_eq!(selection, fixture.selection);
                }
                _ => panic!("completed registration must not be retired"),
            }
        }
        assert!(!fixture.operation_dir().join("retirement.json").exists());
    }
    #[tokio::test]
    async fn concurrent_prepare_and_retire_choose_completed_receipt_or_one_durable_retirement() {
        use std::sync::mpsc;
        for abort_copy in [false, true] {
            let fixture = Arc::new(Fixture::new().await);
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let copying = fixture.clone();
            let worker = std::thread::spawn(move || {
                let mut entered = false;
                copying.with_progress(1100, |_| {
                    if !entered {
                        entered = true;
                        entered_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                    }
                    anyhow::ensure!(!abort_copy, "fixture interrupted copy");
                    Ok(())
                })
            });
            entered_rx.recv().unwrap();
            let resolving = fixture.clone();
            let (resolving_tx, resolving_rx) = mpsc::channel();
            let resolver = std::thread::spawn(move || {
                resolving_tx.send(()).unwrap();
                resolve(
                    resolving.root.path(),
                    &resolving.identity,
                    &resolving.pin,
                    &resolving.intent,
                    1700,
                    &Limits {
                        max_bytes: 1_000_000,
                        cancelled: &resolving.cancelled,
                    },
                )
            });
            resolving_rx.recv().unwrap();
            release_tx.send(()).unwrap();
            let prepared = worker.join().unwrap();
            let result = resolver.join().unwrap().unwrap();
            if abort_copy {
                assert!(prepared.is_err());
                assert!(matches!(result, Resolution::Retired(_)));
                assert!(fixture.run(1100).is_err());
                assert!(!fixture.operation_dir().join("receipt.json").exists());
            } else {
                let expected = prepared.unwrap().receipt;
                match result {
                    Resolution::Prepared { prepared, .. } => assert_eq!(prepared.receipt, expected),
                    _ => panic!("completion must win"),
                }
                assert!(!fixture.operation_dir().join("retirement.json").exists());
            }
        }
    }
}
