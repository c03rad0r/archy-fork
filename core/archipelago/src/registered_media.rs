//! Durable immutable media registration and node-owned rental windows.
//! No RPC endpoint, publication, payment or legacy filename-share mutation.
use crate::{
    container::registration_pin,
    content_purchase::{Contract, Journal, SellerPhase},
    identity::NodeIdentity,
    media_registration::{self, AuthorizedSelection, Intent, Limits, Receipt},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const APP_ID: &str = "indeedhub-api";
const STORE: &str = "registered-media";
const MAX_RECORD: u64 = 64 * 1024;

/// Assertions from the authenticated dashboard approval path, NOT a body that
/// untrusted apps may submit. Bind the exact approved intent/selection in that
/// path before calling this function; request origin alone is not approval.
pub(crate) struct ApprovedSelection<'a> {
    pub authenticated_producer: &'a str,
    pub authenticated_project: &'a str,
    pub intent: &'a Intent,
    pub selection: &'a AuthorizedSelection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    changed_seconds: i64,
    changed_nanos: i64,
}
impl Stamp {
    fn from_file(file: &File) -> Result<Self> {
        let m = file.metadata()?;
        anyhow::ensure!(
            m.is_file() && m.mode() & 0o222 == 0,
            "Registered snapshot is not immutable"
        );
        Ok(Self {
            device: m.dev(),
            inode: m.ino(),
            size: m.len(),
            changed_seconds: m.ctime(),
            changed_nanos: m.ctime_nsec(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registered {
    version: u8,
    receipt: Receipt,
    terms_sha256: String,
    mime_type: String,
    stamp: Stamp,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lease {
    version: u8,
    purchase_id: String,
    buyer_did: String,
    content_id: String,
    contract_hash: String,
    capability_hash: String,
    started_at: u64,
    expires_at: u64,
}

/// The adapter must recheck this deadline while streaming chunks, close the
/// stream at expiry, and must not call open_paid for HTTP HEAD/preflight.
pub(crate) struct OpenedMedia {
    pub file: File,
    pub verification: Arc<crate::rental_readiness::Ready>,
    pub size_bytes: u64,
    pub mime_type: String,
    pub started_at: u64,
    pub expires_at: u64,
}
impl OpenedMedia {
    pub fn still_authorized(&self, now: u64) -> bool {
        now >= self.started_at && now < self.expires_at
    }
}
fn uuid(value: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(value)?;
    anyhow::ensure!(
        parsed.to_string() == value
            && parsed.get_version_num() == 4
            && parsed.get_variant() == uuid::Variant::RFC4122,
        "Invalid registration identifier"
    );
    Ok(())
}
fn registration_id(content_id: &str) -> Result<&str> {
    let id = content_id
        .strip_prefix("registered_")
        .context("Unknown registered content identifier")?;
    uuid(id)?;
    Ok(id)
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn selected_mime(selection: &AuthorizedSelection) -> Result<&'static str> {
    match selection
        .relative_path
        .extension()
        .and_then(|v| v.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4" | "m4v") => Ok("video/mp4"),
        Some("webm") => Ok("video/webm"),
        Some("mov") => Ok("video/quicktime"),
        _ => anyhow::bail!("Select an MP4, WebM or QuickTime video for this registration"),
    }
}
fn terms(receipt: &Receipt) -> Result<String> {
    Ok(hash(&serde_json::to_vec(&serde_json::json!([
        "archipelago.registered-media.terms.v1",
        receipt.node_did,
        receipt.app_audience,
        receipt.producer,
        receipt.project_id,
        receipt.content_id,
        receipt.sha256,
        receipt.size_bytes,
        receipt.price_sats,
        receipt.viewing_seconds,
        receipt.payment_methods
    ]))?))
}
fn constant_equal(a: &str, b: &str) -> bool {
    let a = Sha256::digest(a.as_bytes());
    let b = Sha256::digest(b.as_bytes());
    a.iter()
        .zip(b.iter())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}
struct Held {
    path: PathBuf,
    dir: File,
    _key_lock: Option<File>,
}
fn store(data_dir: &Path, create: bool) -> Result<Held> {
    let path = data_dir.join(STORE);
    if create {
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        if let Err(error) = builder.create(&path) {
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error.into());
            }
        }
        File::open(data_dir)?.sync_all()?;
    }
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)?;
    let m = dir.metadata()?;
    anyhow::ensure!(
        m.uid() == unsafe { libc::geteuid() } && m.mode() & 0o077 == 0,
        "Registered media store must remain private"
    );
    Ok(Held {
        path,
        dir,
        _key_lock: None,
    })
}
fn lock(file: &File) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            break;
        }
        let error = std::io::Error::last_os_error();
        if !matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
        ) {
            return Err(error.into());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "Registered media is busy; retry the same operation"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
fn held(data_dir: &Path, create: bool) -> Result<Held> {
    let held = store(data_dir, create)?;
    lock(&held.dir)?;
    Ok(held)
}
fn keyed(data_dir: &Path, purpose: &str, id: &str) -> Result<Held> {
    uuid(id)?;
    anyhow::ensure!(
        matches!(purpose, "verify" | "lease"),
        "Invalid registered media lock scope"
    );
    let mut held = store(data_dir, false)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(held.path.join(format!("{purpose}-{id}.lock")))?;
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0,
        "Invalid registered media lock storage"
    );
    lock(&file)?;
    held._key_lock = Some(file);
    Ok(held)
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let m = file.metadata()?;
    anyhow::ensure!(
        m.is_file() && m.mode() & 0o077 == 0 && m.len() <= MAX_RECORD,
        "Invalid registered media record storage"
    );
    let mut bytes = Vec::new();
    file.take(MAX_RECORD + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_RECORD,
        "Registered media record too large"
    );
    Ok(Some(serde_json::from_slice(&bytes).context(
        "Registered media state is damaged; preserve it",
    )?))
}
fn persist<T: Serialize>(held: &Held, path: &Path, value: &T) -> Result<()> {
    let mut pending = tempfile::NamedTempFile::new_in(&held.path)?;
    pending.write_all(&serde_json::to_vec(value)?)?;
    pending.as_file().sync_all()?;
    pending
        .persist_noclobber(path)
        .map_err(|e| anyhow::anyhow!("Could not save registered media state: {}", e.error))?;
    held.dir.sync_all()?;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifiedSnapshot {
    version: u8,
    registration_id: String,
    receipt_hash: String,
    sha256: String,
    stamp: Stamp,
}
fn verification(record: &Registered) -> Result<VerifiedSnapshot> {
    Ok(VerifiedSnapshot {
        version: 1,
        registration_id: record.receipt.request_id.clone(),
        receipt_hash: hash(&record.receipt.preimage()?),
        sha256: record.receipt.sha256.clone(),
        stamp: record.stamp.clone(),
    })
}
fn persist_verified(held: &Held, record: &Registered) -> Result<()> {
    let expected = verification(record)?;
    let path = held
        .path
        .join(format!("verified-{}.json", record.receipt.request_id));
    if let Some(saved) = read::<VerifiedSnapshot>(&path)? {
        anyhow::ensure!(
            saved == expected,
            "Immutable snapshot verification binding changed"
        );
    } else if let Err(error) = persist(held, &path, &expected) {
        // Registration retry and missing-cache recovery use different narrow
        // locks. Accept an identical no-replace winner only after independently
        // flushing its file and directory; a failed fsync is never success.
        if read::<VerifiedSnapshot>(&path)?.as_ref() != Some(&expected) {
            return Err(error);
        }
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)?
            .sync_all()?;
        held.dir.sync_all()?;
    }
    Ok(())
}
fn ensure_verified(data_dir: &Path, record: &Registered, file: &mut File) -> Result<()> {
    ensure_verified_for_use(data_dir, record, file, false)
}
fn ensure_verified_for_use(
    data_dir: &Path,
    record: &Registered,
    file: &mut File,
    first_use: bool,
) -> Result<()> {
    // This per-registration lock does not hold the mapping/global directory or
    // any buyer lease lock while hashing. Range opens can reuse the saved
    // verification, but a new lease always checks bytes before starting its clock:
    // same-size writes within a filesystem timestamp tick can share a stamp.
    let held = keyed(data_dir, "verify", &record.receipt.request_id)?;
    let path = held
        .path
        .join(format!("verified-{}.json", record.receipt.request_id));
    let expected = verification(record)?;
    if let Some(saved) = read::<VerifiedSnapshot>(&path)? {
        anyhow::ensure!(
            saved == expected && Stamp::from_file(file)? == record.stamp,
            "Immutable snapshot verification binding changed"
        );
        if !first_use {
            return Ok(());
        }
    }
    // A new lease or missing cache requires the original signed byte hash. Never
    // manufacture a positive cache entry from metadata alone after restart.
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    anyhow::ensure!(
        hex::encode(digest.finalize()) == record.receipt.sha256
            && Stamp::from_file(file)? == record.stamp,
        "Registered snapshot content changed"
    );
    file.seek(SeekFrom::Start(0))?;
    persist_verified(&held, record)
}
fn verify(
    record: &Registered,
    pin: &registration_pin::RegistrationPin,
    identity: &NodeIdentity,
) -> Result<()> {
    let r = &record.receipt;
    uuid(&r.request_id)?;
    let size: u64 = r.size_bytes.parse()?;
    anyhow::ensure!(
        record.version == 1
            && r.version == 1
            && registration_id(&r.content_id)? == r.request_id
            && r.node_did == pin.node_did
            && r.app_audience == pin.app_audience
            && r.size_bytes == size.to_string()
            && record.stamp.size == size
            && matches!(
                record.mime_type.as_str(),
                "video/mp4" | "video/webm" | "video/quicktime"
            )
            && r.viewing_seconds > 0
            && r.viewing_seconds <= 31_536_000
            && record.terms_sha256 == terms(r)?,
        "Registered media binding changed"
    );
    anyhow::ensure!(
        NodeIdentity::verify(&identity.pubkey_hex(), &r.preimage()?, &r.signature)?,
        "Registered media receipt signature failed"
    );
    Ok(())
}
fn open_snapshot(data_dir: &Path, record: &Registered) -> Result<File> {
    open_snapshot_with_stamp(data_dir, record, true)
}
fn open_snapshot_with_stamp(
    data_dir: &Path,
    record: &Registered,
    require_original_stamp: bool,
) -> Result<File> {
    use std::ffi::CString;
    use std::os::fd::FromRawFd;
    // Resolve from the configured data directory in one kernel operation. No
    // component can be replaced with a symlink between separate path checks.
    let id = registration_id(&record.receipt.content_id)?;
    let configured_root = data_dir
        .canonicalize()
        .context("Configured node data directory unavailable")?;
    let root = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(configured_root)?;
    let relative = CString::new(format!("media-registration/{id}/media"))?;
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let how = OpenHow {
        flags: (libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) as u64,
        mode: 0,
        resolve: 0x08 | 0x04, // RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS
    };
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            root.as_raw_fd(),
            relative.as_ptr(),
            &how,
            std::mem::size_of::<OpenHow>(),
        )
    };
    anyhow::ensure!(
        fd >= 0,
        "Could not open immutable registered snapshot: {}",
        std::io::Error::last_os_error()
    );
    let file = unsafe { File::from_raw_fd(fd as i32) };
    anyhow::ensure!(
        Stamp::from_file(&file)?.size == record.stamp.size
            && (!require_original_stamp || Stamp::from_file(&file)? == record.stamp),
        "Registered immutable snapshot changed"
    );
    Ok(file)
}

/// Blocking-worker API. Route must authenticate producer/project and obtain
/// explicit operator consent before construction of ApprovedSelection. Returned
/// receipt now has durable immutable serving terms; it is not yet advertised.
pub(crate) fn register_approved_selection(
    data_dir: &Path,
    cloud_root: &Path,
    identity: &NodeIdentity,
    approved: &ApprovedSelection<'_>,
    now: u64,
    limits: &Limits<'_>,
    progress: impl FnMut(u64) -> Result<()>,
) -> Result<Receipt> {
    anyhow::ensure!(
        approved.authenticated_producer == approved.intent.producer
            && approved.authenticated_project == approved.intent.project_id,
        "Approved media owner or project changed"
    );
    let mime_type = selected_mime(approved.selection)?.to_owned();
    let pin = registration_pin::load_existing(data_dir, APP_ID, identity)?;
    let prepared = media_registration::prepare(
        data_dir,
        cloud_root,
        identity,
        &media_registration::InstallationPin {
            node_did: pin.node_did.clone(),
            app_audience: pin.app_audience.clone(),
        },
        approved.intent,
        approved.selection,
        now,
        limits,
        progress,
    )?;
    commit_prepared(data_dir, identity, &pin, prepared, mime_type)
}

fn commit_prepared(
    data_dir: &Path,
    identity: &NodeIdentity,
    pin: &registration_pin::RegistrationPin,
    prepared: media_registration::PreparedRegistration,
    mime_type: String,
) -> Result<Receipt> {
    let record = Registered {
        version: 1,
        terms_sha256: terms(&prepared.receipt)?,
        mime_type,
        stamp: Stamp::from_file(&prepared.snapshot)?,
        receipt: prepared.receipt,
    };
    verify(&record, &pin, identity)?;
    let held = held(data_dir, true)?;
    // prepare verified the bytes against the signed receipt before returning its
    // descriptor. Preserve that attestation before publishing the serving map.
    persist_verified(&held, &record)?;
    let path = held
        .path
        .join(format!("{}.json", record.receipt.request_id));
    if let Some(saved) = read::<Registered>(&path)? {
        anyhow::ensure!(saved == record, "Registered media operation changed");
    } else {
        persist(&held, &path, &record)?;
    }
    Ok(record.receipt)
}

/// Intent-only resolution preserves original file selection; the request cannot
/// choose a new path. Serving metadata is durable before recovered receipt return.
pub(crate) fn resolve_registration(
    data_dir: &Path,
    identity: &NodeIdentity,
    intent: &Intent,
    authenticated_producer: &str,
    now: u64,
    limits: &Limits<'_>,
) -> Result<serde_json::Value> {
    anyhow::ensure!(
        authenticated_producer == intent.producer,
        "Resolution producer changed"
    );
    let pin = registration_pin::load_existing(data_dir, APP_ID, identity)?;
    match media_registration::resolve(
        data_dir,
        identity,
        &media_registration::InstallationPin {
            node_did: pin.node_did.clone(),
            app_audience: pin.app_audience.clone(),
        },
        intent,
        now,
        limits,
    )? {
        media_registration::Resolution::Prepared {
            prepared,
            selection,
        } => {
            let receipt = commit_prepared(
                data_dir,
                identity,
                &pin,
                prepared,
                selected_mime(&selection)?.into(),
            )?;
            Ok(serde_json::json!({"phase":"completed", "receipt":receipt}))
        }
        media_registration::Resolution::Retired(retirement) => {
            Ok(serde_json::json!({"phase":"retired", "retirement":retirement}))
        }
        media_registration::Resolution::Pending {
            request_id,
            expires_at,
        } => Ok(
            serde_json::json!({"phase":"pending", "requestId":request_id, "expiresAt":expires_at}),
        ),
    }
}

/// No request chooses app scope or storage path. This is an offer prerequisite,
/// not advertisement: the future offer creator must authenticate the peer and
/// bind all returned terms into the purchase contract before seller acceptance.
fn metadata_record(
    data_dir: &Path,
    identity: &NodeIdentity,
    content_id: &str,
) -> Result<Registered> {
    let id = registration_id(content_id)?;
    let pin = registration_pin::load_existing(data_dir, APP_ID, identity)?;
    let held = held(data_dir, false)?;
    let record: Registered = read(&held.path.join(format!("{id}.json")))?
        .context("Registered content is unavailable")?;
    drop(held);
    verify(&record, &pin, identity)?;
    Ok(record)
}
fn chunk_binding(record: &Registered) -> Result<crate::rental_chunk_index::Binding> {
    Ok(crate::rental_chunk_index::Binding {
        content_id: record.receipt.content_id.clone(),
        receipt_sha256: hash(&serde_json::to_vec(&record.receipt)?),
        full_sha256: record.receipt.sha256.clone(),
        size: record.stamp.size,
    })
}
fn readiness_key(data_dir: &Path, record: &Registered) -> Result<String> {
    Ok(format!(
        "{}:{}",
        data_dir.canonicalize()?.display(),
        hash(&serde_json::to_vec(&record.receipt)?)
    ))
}
/// Signed metadata only: no whole-file scan or lease is permitted here.
pub(crate) fn registered_metadata(
    data_dir: &Path,
    identity: &NodeIdentity,
    content_id: &str,
) -> Result<(Receipt, String)> {
    let record = metadata_record(data_dir, identity, content_id)?;
    Ok((record.receipt, record.terms_sha256))
}
/// Starts/reuses bounded background verification, before quoting or playing.
/// The signed index survives restart; range requests never invoke this scan.
pub(crate) fn prepare_registered(
    data_dir: PathBuf,
    identity: Arc<NodeIdentity>,
    content_id: &str,
    retry: bool,
) -> Result<crate::rental_readiness::Status> {
    let record = metadata_record(&data_dir, &identity, content_id)?;
    let binding = chunk_binding(&record)?;
    let key = readiness_key(&data_dir, &record)?;
    let manager = crate::rental_readiness::shared();
    if retry {
        manager.retry_failed(&key, &binding)?;
    }
    manager.prepare(key, binding.clone(), move |progress| {
        let held = keyed(&data_dir, "verify", &record.receipt.request_id)?;
        let path = held
            .path
            .join(format!("chunks-{}.bin", record.receipt.request_id));
        let mut file = open_snapshot_with_stamp(&data_dir, &record, false)?;
        if let Ok(Some(index)) = crate::rental_chunk_index::Index::load(&path, &binding, &identity)
        {
            return Ok(index);
        }
        // A damaged derived index is not evidence about the original bytes.
        // Rebuild only after a complete scan matches the signed producer SHA;
        // this avoids making an editable cache a permanent recovery dead end.
        let index = crate::rental_chunk_index::Index::scan(&mut file, binding, |n| {
            progress.store(n, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })?;
        index.save(&path, &identity)?;
        Ok(index)
    })
}
#[derive(Clone, Serialize)]
pub(crate) struct RentalWindow {
    pub started_at: u64,
    pub expires_at: u64,
}
fn verify_lease(
    lease: &Lease,
    contract: &Contract,
    capability: &str,
    duration: u64,
) -> Result<RentalWindow> {
    anyhow::ensure!(
        lease.version == 1
            && lease.purchase_id == contract.id
            && lease.buyer_did == contract.buyer_did
            && lease.content_id == contract.content_id
            && lease.contract_hash == contract.context_hash()?
            && lease.capability_hash == hash(capability.as_bytes())
            && lease.started_at.checked_add(duration) == Some(lease.expires_at),
        "Persisted rental terms changed"
    );
    Ok(RentalWindow {
        started_at: lease.started_at,
        expires_at: lease.expires_at,
    })
}
fn verify_settled_terms(record: &Registered, contract: &Contract) -> Result<()> {
    anyhow::ensure!(
        contract.content_sha256 == record.receipt.sha256
            && contract.content_size == record.stamp.size
            && contract.terms_sha256 == record.terms_sha256
            && record.receipt.price_sats > 0
            && record
                .receipt
                .payment_methods
                .iter()
                .any(|method| method == "cashu")
            && contract.minimum_net_sats == record.receipt.price_sats,
        "Settled purchase does not match registered immutable terms"
    );
    Ok(())
}
/// Only explicit authenticated Start invokes this commit. Losing its response
/// cannot reset the original clock; repeated Start returns the same window.
fn start_ready(
    data_dir: &Path,
    identity: &NodeIdentity,
    contract: &Contract,
    capability: &str,
    ready_id: &str,
    now: impl Fn() -> Result<u64>,
) -> Result<RentalWindow> {
    let record = metadata_record(data_dir, identity, &contract.content_id)?;
    verify_settled_terms(&record, contract)?;
    {
        let held = keyed(data_dir, "lease", &contract.id)?;
        if let Some(lease) = read::<Lease>(&held.path.join(format!("lease-{}.json", contract.id)))?
        {
            return verify_lease(&lease, contract, capability, record.receipt.viewing_seconds);
        }
    }
    let ready = crate::rental_readiness::shared().ready(
        &readiness_key(data_dir, &record)?,
        &chunk_binding(&record)?,
        Some(ready_id),
    )?;
    let mut file = open_snapshot_with_stamp(data_dir, &record, false)?;
    // A bounded actual byte read before the first clock. Later chunks are
    // verified immediately before delivery, including seek/range requests.
    ready.index.read_slice(&mut file, 0, 1)?;
    let held = keyed(data_dir, "lease", &contract.id)?;
    let path = held.path.join(format!("lease-{}.json", contract.id));
    if let Some(lease) = read::<Lease>(&path)? {
        return verify_lease(&lease, contract, capability, record.receipt.viewing_seconds);
    }
    let started_at = now()?;
    let expires_at = started_at
        .checked_add(record.receipt.viewing_seconds)
        .context("Rental expiry overflow")?;
    let lease = Lease {
        version: 1,
        purchase_id: contract.id.clone(),
        buyer_did: contract.buyer_did.clone(),
        content_id: contract.content_id.clone(),
        contract_hash: contract.context_hash()?,
        capability_hash: hash(capability.as_bytes()),
        started_at,
        expires_at,
    };
    persist(&held, &path, &lease)?;
    Ok(RentalWindow {
        started_at,
        expires_at,
    })
}

pub(crate) fn registered_terms(
    data_dir: &Path,
    identity: &NodeIdentity,
    content_id: &str,
) -> Result<(Receipt, String)> {
    let record = metadata_record(data_dir, identity, content_id)?;
    let _ready = crate::rental_readiness::shared().ready(
        &readiness_key(data_dir, &record)?,
        &chunk_binding(&record)?,
        None,
    )?;
    let _file = open_snapshot_with_stamp(data_dir, &record, false)?;
    Ok((record.receipt, record.terms_sha256))
}

/// Only authenticated peer GET/range routes may call this. The capability is
/// checked against this node's durable seller journal; client receipts do not
/// establish payment. A lease is persisted before any bytes can be returned.
async fn paid_contract(
    data_dir: &Path,
    identity: &NodeIdentity,
    content_id: &str,
    purchase_id: &str,
    authenticated_buyer: &str,
    capability: &str,
) -> Result<Contract> {
    uuid(&purchase_id)?;
    registration_id(&content_id)?;
    anyhow::ensure!(
        capability.len() == 64 && capability.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid delivery capability"
    );
    let contract = {
        let journal = Journal::open(data_dir).await?;
        let record = journal
            .seller(&purchase_id)
            .await?
            .context("Seller settlement is not durable")?;
        anyhow::ensure!(
            record.contract.buyer_did == authenticated_buyer
                && record.contract.seller_did == identity.did_key()?
                && record.contract.content_id == content_id,
            "Paid content does not belong to this authenticated purchase"
        );
        let receipt = match record.phase {
            SellerPhase::ReceiptSaved(receipt) => receipt,
            _ => anyhow::bail!("Seller receipt is not durable"),
        };
        anyhow::ensure!(
            constant_equal(capability, &receipt.capability),
            "Delivery capability does not match this purchase"
        );
        record.contract
    };
    Ok(contract)
}
pub(crate) async fn prepare_paid(
    data_dir: PathBuf,
    identity: Arc<NodeIdentity>,
    content_id: String,
    purchase_id: String,
    buyer: String,
    capability: String,
    retry: bool,
) -> Result<crate::rental_readiness::Status> {
    paid_contract(
        &data_dir,
        &identity,
        &content_id,
        &purchase_id,
        &buyer,
        &capability,
    )
    .await?;
    tokio::task::spawn_blocking(move || prepare_registered(data_dir, identity, &content_id, retry))
        .await?
}
pub(crate) async fn paid_window(
    data_dir: &Path,
    identity: &NodeIdentity,
    content_id: &str,
    purchase_id: &str,
    buyer: &str,
    capability: &str,
) -> Result<Option<RentalWindow>> {
    let contract = paid_contract(
        data_dir,
        identity,
        content_id,
        purchase_id,
        buyer,
        capability,
    )
    .await?;
    let record = metadata_record(data_dir, identity, content_id)?;
    verify_settled_terms(&record, &contract)?;
    let held = keyed(data_dir, "lease", purchase_id)?;
    read::<Lease>(&held.path.join(format!("lease-{purchase_id}.json")))?
        .map(|lease| {
            verify_lease(
                &lease,
                &contract,
                capability,
                record.receipt.viewing_seconds,
            )
        })
        .transpose()
}
pub(crate) async fn start_paid(
    data_dir: PathBuf,
    identity: Arc<NodeIdentity>,
    content_id: String,
    purchase_id: String,
    buyer: String,
    capability: String,
    ready_id: String,
) -> Result<RentalWindow> {
    let contract = paid_contract(
        &data_dir,
        &identity,
        &content_id,
        &purchase_id,
        &buyer,
        &capability,
    )
    .await?;
    tokio::task::spawn_blocking(move || {
        start_ready(
            &data_dir,
            &identity,
            &contract,
            &capability,
            &ready_id,
            || u64::try_from(chrono::Utc::now().timestamp()).context("Invalid node clock"),
        )
    })
    .await?
}
pub(crate) async fn open_paid(
    data_dir: PathBuf,
    identity: Arc<NodeIdentity>,
    content_id: String,
    purchase_id: String,
    authenticated_buyer: String,
    capability: String,
) -> Result<OpenedMedia> {
    let contract = paid_contract(
        &data_dir,
        &identity,
        &content_id,
        &purchase_id,
        &authenticated_buyer,
        &capability,
    )
    .await?;
    tokio::task::spawn_blocking(move || {
        open_started(&data_dir, &identity, &contract, &capability, || {
            u64::try_from(chrono::Utc::now().timestamp()).context("Invalid node clock")
        })
    })
    .await?
}
fn open_started(
    data_dir: &Path,
    identity: &NodeIdentity,
    contract: &Contract,
    capability: &str,
    now: impl Fn() -> Result<u64>,
) -> Result<OpenedMedia> {
    let record = metadata_record(data_dir, identity, &contract.content_id)?;
    verify_settled_terms(&record, contract)?;
    let lease = {
        let held = keyed(data_dir, "lease", &contract.id)?;
        read::<Lease>(&held.path.join(format!("lease-{}.json", contract.id)))?
            .context("Explicit Start is required; no rental was started")?
    };
    let window = verify_lease(&lease, contract, capability, record.receipt.viewing_seconds)?;
    let instant = now()?;
    anyhow::ensure!(
        instant >= window.started_at && instant < window.expires_at,
        "Rental expired or node clock moved backwards"
    );
    let verification = crate::rental_readiness::shared().ready(
        &readiness_key(data_dir, &record)?,
        &chunk_binding(&record)?,
        None,
    )?;
    let file = open_snapshot_with_stamp(data_dir, &record, false)?;
    Ok(OpenedMedia {
        file,
        verification,
        size_bytes: record.stamp.size,
        mime_type: record.mime_type,
        started_at: window.started_at,
        expires_at: window.expires_at,
    })
}
// Existing clock/receipt invariant fixtures exercise an explicit local start;
// public prepare/start/open separation is tested independently below.
#[cfg(test)]
fn open_settled(
    data_dir: &Path,
    identity: &NodeIdentity,
    contract: &Contract,
    capability: &str,
    now: impl Fn() -> Result<u64>,
) -> Result<OpenedMedia> {
    let id = registration_id(&contract.content_id)?;
    let pin = registration_pin::load_existing(data_dir, APP_ID, identity)?;
    let held = held(data_dir, false)?;
    let record: Registered = read(&held.path.join(format!("{id}.json")))?
        .context("Registered content is unavailable")?;
    drop(held);
    verify(&record, &pin, identity)?;
    anyhow::ensure!(
        contract.content_sha256 == record.receipt.sha256
            && contract.content_size == record.stamp.size
            && contract.terms_sha256 == record.terms_sha256
            && record.receipt.price_sats > 0
            && record
                .receipt
                .payment_methods
                .iter()
                .any(|method| method == "cashu")
            && contract.minimum_net_sats == record.receipt.price_sats,
        "Settled purchase does not match registered immutable terms"
    );
    // Open before recording a first use: unreadable or altered media does not
    // start a rental. No bytes leave this descriptor until the lease is durable.
    let mut file = open_snapshot(data_dir, &record)?;
    let lease_path = data_dir
        .join(STORE)
        .join(format!("lease-{}.json", contract.id));
    let first_use = read::<Lease>(&lease_path)?.is_none();
    ensure_verified_for_use(data_dir, &record, &mut file, first_use)?;
    let held = keyed(data_dir, "lease", &contract.id)?;
    let path = held.path.join(format!("lease-{}.json", contract.id));
    let contract_hash = contract.context_hash()?;
    let capability_hash = hash(capability.as_bytes());
    let lease = if let Some(lease) = read::<Lease>(&path)? {
        anyhow::ensure!(
            lease.version == 1
                && lease.purchase_id == contract.id
                && lease.buyer_did == contract.buyer_did
                && lease.content_id == contract.content_id
                && lease.contract_hash == contract_hash
                && lease.capability_hash == capability_hash
                && lease.started_at.checked_add(record.receipt.viewing_seconds)
                    == Some(lease.expires_at),
            "Persisted rental terms changed"
        );
        lease
    } else {
        let now = now()?;
        let expires_at = now
            .checked_add(record.receipt.viewing_seconds)
            .context("Rental expiry overflow")?;
        let lease = Lease {
            version: 1,
            purchase_id: contract.id.clone(),
            buyer_did: contract.buyer_did.clone(),
            content_id: contract.content_id.clone(),
            contract_hash,
            capability_hash,
            started_at: now,
            expires_at,
        };
        persist(&held, &path, &lease)?;
        lease
    };
    let now = now()?;
    anyhow::ensure!(
        now >= lease.started_at && now < lease.expires_at,
        "Rental expired or node clock moved backwards"
    );
    let index =
        crate::rental_chunk_index::Index::scan(&mut file, chunk_binding(&record)?, |_| Ok(()))?;
    let verification = crate::rental_readiness::Ready::fixture(index);
    Ok(OpenedMedia {
        file,
        verification,
        size_bytes: record.stamp.size,
        mime_type: record.mime_type,
        started_at: lease.started_at,
        expires_at: lease.expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        cashu::{CashuToken, Proof},
        ecash::EcashNetwork,
    };
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::AtomicBool;
    struct Fixture {
        root: tempfile::TempDir,
        identity: Arc<NodeIdentity>,
        intent: Intent,
        selection: AuthorizedSelection,
        cancel: AtomicBool,
    }
    impl Fixture {
        async fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let identity_dir = root.path().join("identity");
            std::fs::create_dir(&identity_dir).unwrap();
            // Public fixed test key; never invoke node identity generation.
            std::fs::write(identity_dir.join("node_key"), [7u8; 32]).unwrap();
            std::fs::set_permissions(
                identity_dir.join("node_key"),
                std::fs::Permissions::from_mode(0o600),
            )
            .unwrap();
            let identity = Arc::new(NodeIdentity::load_existing(&identity_dir).await.unwrap());
            let pin =
                registration_pin::ensure_for_installation(root.path(), APP_ID, &identity).unwrap();
            let cloud = root.path().join("cloud");
            std::fs::create_dir(&cloud).unwrap();
            std::fs::write(cloud.join("film.mp4"), b"original immutable movie").unwrap();
            Self {
                root,
                identity,
                intent: Intent {
                    version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    nonce: "ab".repeat(32),
                    app_audience: pin.app_audience,
                    node_did: pin.node_did,
                    producer: "cd".repeat(32),
                    project_id: "film-project".into(),
                    price_sats: 8,
                    viewing_seconds: 60,
                    created_at: 1000,
                    expires_at: 1600,
                },
                selection: AuthorizedSelection {
                    relative_path: "film.mp4".into(),
                    payment_methods: vec!["cashu".into()],
                },
                cancel: AtomicBool::new(false),
            }
        }
        fn register(&self, now: u64) -> Result<Receipt> {
            register_approved_selection(
                self.root.path(),
                &self.root.path().join("cloud"),
                &self.identity,
                &ApprovedSelection {
                    authenticated_producer: &self.intent.producer,
                    authenticated_project: &self.intent.project_id,
                    intent: &self.intent,
                    selection: &self.selection,
                },
                now,
                &Limits {
                    max_bytes: 1_000_000,
                    cancelled: &self.cancel,
                },
                |_| Ok(()),
            )
        }
        async fn ready(&self, receipt: &Receipt) -> String {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    match prepare_registered(
                        self.root.path().into(),
                        self.identity.clone(),
                        &receipt.content_id,
                        false,
                    )
                    .unwrap()
                    {
                        crate::rental_readiness::Status::Ready { ready_id, .. } => return ready_id,
                        crate::rental_readiness::Status::Preparing { .. } => {
                            tokio::time::sleep(std::time::Duration::from_millis(5)).await
                        }
                        crate::rental_readiness::Status::Unavailable => {
                            panic!("fixture verification failed")
                        }
                    }
                }
            })
            .await
            .expect("bounded fixture preparation")
        }
        fn contract(&self, receipt: &Receipt) -> Contract {
            Contract {
                version: 1,
                id: uuid::Uuid::new_v4().to_string(),
                buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([1; 32])).unwrap(),
                seller_did: self.identity.did_key().unwrap(),
                content_id: receipt.content_id.clone(),
                content_sha256: receipt.sha256.clone(),
                content_size: receipt.size_bytes.parse().unwrap(),
                terms_sha256: terms(receipt).unwrap(),
                network: EcashNetwork::Mainnet,
                mint_url: "https://fixture.invalid".into(),
                gross_token_sats: 8,
                minimum_net_sats: 8,
                offered_at: 1000,
                expires_at: 1600,
            }
        }
        async fn settle_fixture(&self, contract: &Contract) -> crate::content_purchase::Receipt {
            // Local journal state fixture only, no wallet/mint call or claimed
            // live settlement. Public open_paid must require this durable state.
            let key = bitcoin::secp256k1::SecretKey::from_slice(&[7; 32]).unwrap();
            let point = bitcoin::secp256k1::PublicKey::from_secret_key(
                &bitcoin::secp256k1::Secp256k1::new(),
                &key,
            )
            .to_string();
            let token = CashuToken::new(
                &contract.mint_url,
                vec![Proof {
                    id: "0011223344556677".into(),
                    amount: 8,
                    secret: "fixture-incoming".into(),
                    c: point,
                }],
            )
            .serialize()
            .unwrap();
            let journal = Journal::open(self.root.path()).await.unwrap();
            journal.prepare_seller(contract, 1200).await.unwrap();
            journal
                .record_incoming_token(contract, &token)
                .await
                .unwrap();
            journal.record_settlement(contract, 8).await.unwrap();
            journal.issue_receipt(contract).await.unwrap()
        }
    }
    #[test]
    fn independent_nodejs_terms_preimage_and_digest_match() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("registered_media/fixtures/terms-v1.json")).unwrap();
        let receipt: Receipt = serde_json::from_value(fixture["receipt"].clone()).unwrap();
        assert_eq!(
            terms(&receipt).unwrap(),
            fixture["sha256"].as_str().unwrap()
        );
        assert_eq!(
            hash(fixture["preimageUtf8"].as_str().unwrap().as_bytes()),
            fixture["sha256"].as_str().unwrap()
        );
        let mut changed = receipt;
        changed.viewing_seconds += 1;
        assert_ne!(
            terms(&changed).unwrap(),
            fixture["sha256"].as_str().unwrap()
        );
    }
    #[tokio::test]
    async fn approved_mapping_is_durable_and_retries_after_source_removal_preserve_terms() {
        let mut fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        let mapping = fixture
            .root
            .path()
            .join(STORE)
            .join(format!("{}.json", receipt.request_id));
        let original = std::fs::read(&mapping).unwrap();
        // Model interruption between durable registration receipt and serving
        // mapping: retry reconstructs only the exact original immutable mapping.
        std::fs::rename(&mapping, fixture.root.path().join("fixture-mapping-backup")).unwrap();
        assert_eq!(fixture.register(2000).unwrap(), receipt);
        assert_eq!(std::fs::read(&mapping).unwrap(), original);
        std::fs::remove_file(fixture.root.path().join("cloud/film.mp4")).unwrap();
        assert_eq!(fixture.register(2000).unwrap(), receipt);
        assert_eq!(std::fs::read(&mapping).unwrap(), original);
        fixture.ready(&receipt).await;
        let found =
            registered_terms(fixture.root.path(), &fixture.identity, &receipt.content_id).unwrap();
        assert_eq!(found, (receipt.clone(), terms(&receipt).unwrap()));
        fixture.intent.price_sats = 9;
        assert!(fixture.register(1200).is_err());
        assert_eq!(std::fs::read(&mapping).unwrap(), original);
    }
    #[tokio::test]
    async fn wrong_owner_and_unprovisioned_or_changed_app_pin_reject_before_snapshot() {
        let fixture = Fixture::new().await;
        let rejected = register_approved_selection(
            fixture.root.path(),
            &fixture.root.path().join("cloud"),
            &fixture.identity,
            &ApprovedSelection {
                authenticated_producer: "foreign",
                authenticated_project: &fixture.intent.project_id,
                intent: &fixture.intent,
                selection: &fixture.selection,
            },
            1100,
            &Limits {
                max_bytes: 1000,
                cancelled: &fixture.cancel,
            },
            |_| Ok(()),
        );
        assert!(rejected.is_err());
        assert!(!fixture.root.path().join("media-registration").exists());
        let mut wrong = fixture.intent.clone();
        wrong.app_audience = uuid::Uuid::new_v4().to_string();
        assert!(register_approved_selection(
            fixture.root.path(),
            &fixture.root.path().join("cloud"),
            &fixture.identity,
            &ApprovedSelection {
                authenticated_producer: &wrong.producer,
                authenticated_project: &wrong.project_id,
                intent: &wrong,
                selection: &fixture.selection
            },
            1100,
            &Limits {
                max_bytes: 1000,
                cancelled: &fixture.cancel
            },
            |_| Ok(())
        )
        .is_err());
        assert!(!fixture.root.path().join("media-registration").exists());
        std::fs::remove_file(
            fixture
                .root
                .path()
                .join("app-registration-pins/indeedhub-api.json"),
        )
        .unwrap();
        assert!(fixture.register(1100).is_err());
        assert!(!fixture.root.path().join("media-registration").exists());
    }
    #[tokio::test]
    async fn rental_reopens_keep_first_window_and_reject_expiry_clock_rollback_and_changed_receipt()
    {
        let fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        let contract = fixture.contract(&receipt);
        let first = open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(2000),
        )
        .unwrap();
        assert_eq!((first.started_at, first.expires_at), (2000, 2060));
        let mut again = open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(2030),
        )
        .unwrap();
        assert_eq!((again.started_at, again.expires_at), (2000, 2060));
        let mut bytes = Vec::new();
        again.file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"original immutable movie");
        assert!(again.still_authorized(2059));
        assert!(!again.still_authorized(2060));
        assert!(!again.still_authorized(1999));
        assert!(open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(2060)
        )
        .is_err());
        assert!(open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(1999)
        )
        .is_err());
        assert!(open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"cd".repeat(32),
            || Ok(2030)
        )
        .is_err());
    }
    #[tokio::test]
    async fn public_open_requires_matching_durable_settlement_and_peer_capability() {
        let fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        let contract = fixture.contract(&receipt);
        assert!(open_paid(
            fixture.root.path().into(),
            fixture.identity.clone(),
            receipt.content_id.clone(),
            contract.id.clone(),
            contract.buyer_did.clone(),
            "ab".repeat(32)
        )
        .await
        .is_err());
        let settled = fixture.settle_fixture(&contract).await;
        assert!(open_paid(
            fixture.root.path().into(),
            fixture.identity.clone(),
            receipt.content_id.clone(),
            contract.id.clone(),
            contract.seller_did.clone(),
            settled.capability.clone()
        )
        .await
        .is_err());
        assert!(open_paid(
            fixture.root.path().into(),
            fixture.identity.clone(),
            receipt.content_id.clone(),
            contract.id.clone(),
            contract.buyer_did.clone(),
            "ab".repeat(32)
        )
        .await
        .is_err());
        assert!(!fixture
            .root
            .path()
            .join(STORE)
            .join(format!("lease-{}.json", contract.id))
            .exists());
        let ready_id = fixture.ready(&receipt).await;
        // Preparation, settlement and a GET are not permission to start the clock.
        assert!(open_paid(
            fixture.root.path().into(),
            fixture.identity.clone(),
            receipt.content_id.clone(),
            contract.id.clone(),
            contract.buyer_did.clone(),
            settled.capability.clone()
        )
        .await
        .is_err());
        assert!(paid_window(
            fixture.root.path(),
            &fixture.identity,
            &receipt.content_id,
            &contract.id,
            &contract.buyer_did,
            &settled.capability
        )
        .await
        .unwrap()
        .is_none());
        let window = start_paid(
            fixture.root.path().into(),
            fixture.identity.clone(),
            receipt.content_id.clone(),
            contract.id.clone(),
            contract.buyer_did.clone(),
            settled.capability.clone(),
            ready_id,
        )
        .await
        .unwrap();
        // Lost Start response: a retry replays the durable original window even
        // if its ephemeral readiness identifier no longer exists after restart.
        let replay = start_paid(
            fixture.root.path().into(),
            fixture.identity.clone(),
            receipt.content_id.clone(),
            contract.id.clone(),
            contract.buyer_did.clone(),
            settled.capability.clone(),
            uuid::Uuid::new_v4().to_string(),
        )
        .await
        .unwrap();
        assert_eq!(
            (window.started_at, window.expires_at),
            (replay.started_at, replay.expires_at)
        );
        let opened = open_paid(
            fixture.root.path().into(),
            fixture.identity.clone(),
            receipt.content_id.clone(),
            contract.id.clone(),
            contract.buyer_did.clone(),
            settled.capability.clone(),
        )
        .await
        .unwrap();
        assert_eq!(opened.expires_at - opened.started_at, 60);
        assert_eq!(opened.size_bytes, 24);
        for offset in [0, 8, 23] {
            let mut range = open_paid(
                fixture.root.path().into(),
                fixture.identity.clone(),
                receipt.content_id.clone(),
                contract.id.clone(),
                contract.buyer_did.clone(),
                settled.capability.clone(),
            )
            .await
            .unwrap();
            assert_eq!(
                range
                    .verification
                    .index
                    .read_slice(&mut range.file, offset, 1)
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(range.started_at, window.started_at);
        }
        assert_eq!(
            crate::rental_chunk_index::scans(&receipt.content_id),
            1,
            "quote preparation and repeated GET/ranges share one original SHA scan"
        );
    }
    #[tokio::test]
    async fn altered_snapshot_or_overflow_never_creates_first_rental() {
        let fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        let contract = fixture.contract(&receipt);
        assert!(open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(u64::MAX)
        )
        .is_err());
        let lease = fixture
            .root
            .path()
            .join(STORE)
            .join(format!("lease-{}.json", contract.id));
        assert!(!lease.exists());
        let media = fixture
            .root
            .path()
            .join("media-registration")
            .join(&receipt.request_id)
            .join("media");
        std::fs::set_permissions(&media, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&media, b"changed immutable movie!").unwrap();
        std::fs::set_permissions(&media, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert!(open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(2000)
        )
        .is_err());
        assert!(!lease.exists());
        // Independently exercise SHA256 failure, not only the inode/ctime gate.
        // This local fixture updates only the unsigned filesystem stamp; the
        // original signed content hash remains unchanged and must still win.
        let mapping = fixture
            .root
            .path()
            .join(STORE)
            .join(format!("{}.json", receipt.request_id));
        let mut record: Registered = read(&mapping).unwrap().unwrap();
        record.stamp = Stamp::from_file(&File::open(&media).unwrap()).unwrap();
        // Model an indistinguishable metadata stamp deterministically: both
        // unsigned cache/mapping stamps match, while signed bytes do not. A
        // positive metadata cache must not authorize an offer or first lease.
        std::fs::write(&mapping, serde_json::to_vec(&record).unwrap()).unwrap();
        let verified = fixture
            .root
            .path()
            .join(STORE)
            .join(format!("verified-{}.json", receipt.request_id));
        std::fs::write(
            &verified,
            serde_json::to_vec(&verification(&record).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(
            registered_terms(fixture.root.path(), &fixture.identity, &receipt.content_id).is_err()
        );
        assert!(open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(2000)
        )
        .is_err());
        assert!(!lease.exists());
        std::fs::remove_file(
            fixture
                .root
                .path()
                .join(STORE)
                .join(format!("verified-{}.json", receipt.request_id)),
        )
        .unwrap();
        std::fs::write(&mapping, serde_json::to_vec(&record).unwrap()).unwrap();
        let error = open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(2000),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("snapshot content changed"));
        assert!(!lease.exists());
    }
    #[tokio::test]
    async fn concurrent_first_opens_share_one_persisted_window() {
        let fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        let contract = fixture.contract(&receipt);
        let threads: Vec<_> = [2000u64, 2000]
            .into_iter()
            .map(|now| {
                let root = fixture.root.path().to_owned();
                let identity = fixture.identity.clone();
                let contract = contract.clone();
                std::thread::spawn(move || {
                    let opened =
                        open_settled(&root, &identity, &contract, &"ab".repeat(32), || Ok(now))
                            .unwrap();
                    (opened.started_at, opened.expires_at)
                })
            })
            .collect();
        let windows: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(windows[0], windows[1]);
        assert_eq!(windows[0].1 - windows[0].0, 60);
    }
    #[tokio::test]
    async fn verification_and_purchase_locks_do_not_hold_other_content_or_mapping_locks() {
        let fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        let verify_lock = keyed(fixture.root.path(), "verify", &receipt.request_id).unwrap();
        let other_content = keyed(
            fixture.root.path(),
            "verify",
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        let purchase = keyed(
            fixture.root.path(),
            "lease",
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        let directory = held(fixture.root.path(), false).unwrap();
        // All locks remain held at once: a blocked/hash-heavy registration does
        // not lock another video, another purchase, or the metadata directory.
        drop((verify_lock, other_content, purchase, directory));
    }
    #[tokio::test]
    async fn registration_cache_survives_reconstruction_and_missing_cache_rehashes_before_rental() {
        let fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        let contract = fixture.contract(&receipt);
        let cache = fixture
            .root
            .path()
            .join(STORE)
            .join(format!("verified-{}.json", receipt.request_id));
        let original = std::fs::read(&cache).unwrap();
        let record: Registered = read(
            &fixture
                .root
                .path()
                .join(STORE)
                .join(format!("{}.json", receipt.request_id)),
        )
        .unwrap()
        .unwrap();
        let mut file = open_snapshot(fixture.root.path(), &record).unwrap();
        // A cached check does not scan or reposition the verified descriptor.
        file.seek(SeekFrom::Start(3)).unwrap();
        ensure_verified(fixture.root.path(), &record, &mut file).unwrap();
        assert_eq!(file.stream_position().unwrap(), 3);
        std::fs::remove_file(&cache).unwrap();
        let opened = open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(2000),
        )
        .unwrap();
        assert_eq!(opened.started_at, 2000);
        assert_eq!(std::fs::read(cache).unwrap(), original);
    }
    #[tokio::test]
    async fn mismatched_verification_cache_is_preserved_and_cannot_start_rental() {
        let fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        let contract = fixture.contract(&receipt);
        let path = fixture
            .root
            .path()
            .join(STORE)
            .join(format!("verified-{}.json", receipt.request_id));
        let mut cache: VerifiedSnapshot = read(&path).unwrap().unwrap();
        cache.sha256 = "ff".repeat(32);
        let changed = serde_json::to_vec(&cache).unwrap();
        std::fs::write(&path, &changed).unwrap();
        assert!(open_settled(
            fixture.root.path(),
            &fixture.identity,
            &contract,
            &"ab".repeat(32),
            || Ok(2000)
        )
        .is_err());
        assert_eq!(std::fs::read(path).unwrap(), changed);
        assert!(!fixture
            .root
            .path()
            .join(STORE)
            .join(format!("lease-{}.json", contract.id))
            .exists());
    }
    #[tokio::test]
    async fn signed_index_preparation_is_lease_free_and_unsigned_cache_cannot_change_it() {
        let fixture = Fixture::new().await;
        let receipt = fixture.register(1100).unwrap();
        assert!(
            registered_terms(fixture.root.path(), &fixture.identity, &receipt.content_id).is_err()
        );
        let ready_id = fixture.ready(&receipt).await;
        let record =
            metadata_record(fixture.root.path(), &fixture.identity, &receipt.content_id).unwrap();
        let index_path = fixture
            .root
            .path()
            .join(STORE)
            .join(format!("chunks-{}.bin", receipt.request_id));
        let index = crate::rental_chunk_index::Index::load(
            &index_path,
            &chunk_binding(&record).unwrap(),
            &fixture.identity,
        )
        .unwrap()
        .unwrap();
        assert_eq!(index.binding.full_sha256, receipt.sha256);
        let cache = fixture
            .root
            .path()
            .join(STORE)
            .join(format!("verified-{}.json", receipt.request_id));
        std::fs::write(cache, b"untrusted corrupt old metadata").unwrap();
        assert_eq!(
            registered_terms(fixture.root.path(), &fixture.identity, &receipt.content_id)
                .unwrap()
                .0,
            receipt
        );
        assert_eq!(fixture.ready(&receipt).await, ready_id);
        assert!(std::fs::read_dir(fixture.root.path().join(STORE))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("lease-")));
        let mut bytes = std::fs::read(&index_path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        std::fs::write(&index_path, bytes).unwrap();
        assert!(crate::rental_chunk_index::Index::load(
            &index_path,
            &chunk_binding(&record).unwrap(),
            &fixture.identity
        )
        .is_err());
        crate::rental_readiness::shared()
            .forget_for_restart(&readiness_key(fixture.root.path(), &record).unwrap());
        let replacement = fixture.ready(&receipt).await;
        assert_ne!(replacement, ready_id);
        assert_eq!(crate::rental_chunk_index::scans(&receipt.content_id), 2);
        assert!(crate::rental_chunk_index::Index::load(
            &index_path,
            &chunk_binding(&record).unwrap(),
            &fixture.identity
        )
        .unwrap()
        .is_some());
        assert!(std::fs::read_dir(fixture.root.path().join(STORE))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("lease-")));
    }
}
