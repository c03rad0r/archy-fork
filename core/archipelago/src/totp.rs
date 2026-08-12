use anyhow::{Context, Result};
use argon2::{Argon2, Params};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Secret, TOTP};
use zeroize::Zeroize;

const ARGON2_M_COST: u32 = 65536; // 64 MiB
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 4;
const ARGON2_OUTPUT_LEN: usize = 32;

const BACKUP_CODE_COUNT: usize = 8;
const BACKUP_CODE_LEN: usize = 8; // 8 alphanumeric chars

/// Encrypted TOTP data stored in user.json. The secret never touches disk in plaintext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpData {
    /// Argon2id salt for KEK derivation (base64)
    pub kek_salt: String,
    /// Nonce for MEK encryption (base64)
    pub mek_nonce: String,
    /// MEK encrypted under KEK via ChaCha20-Poly1305 (base64)
    pub encrypted_mek: String,
    /// Nonce for TOTP secret encryption (base64)
    pub secret_nonce: String,
    /// TOTP secret encrypted under MEK via ChaCha20-Poly1305 (base64)
    pub encrypted_secret: String,
    /// Hashed backup codes (bcrypt), one-time use
    pub backup_codes: Vec<String>,
    /// Recently used TOTP time steps for replay protection
    #[serde(default)]
    pub used_steps: Vec<i64>,
}

/// Result of setup: encrypted data for storage + plaintext for display (shown once).
pub struct SetupResult {
    pub totp_data: TotpData,
    pub secret_base32: String,
    pub qr_svg: String,
    pub backup_codes: Vec<String>,
}

/// Generate a TOTP setup. Returns encrypted data + display values.
pub fn setup(password: &str) -> Result<SetupResult> {
    // Generate the raw TOTP secret (20 bytes = 160 bits, standard for SHA1)
    let mut totp_secret = vec![0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut totp_secret);

    // Generate MEK (Master Encryption Key)
    let mut mek = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut mek);

    // Derive KEK from password via Argon2id
    let mut kek_salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut kek_salt);
    let mut kek = derive_kek(password, &kek_salt)?;

    // Encrypt MEK under KEK
    let mut mek_nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut mek_nonce_bytes);
    let encrypted_mek = encrypt_chacha(&kek, &mek_nonce_bytes, &mek)?;

    // Encrypt TOTP secret under MEK
    let mut secret_nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut secret_nonce_bytes);
    let encrypted_secret = encrypt_chacha(&mek, &secret_nonce_bytes, &totp_secret)?;

    // Generate backup codes
    let (plaintext_codes, hashed_codes) = generate_backup_codes()?;

    // Encode the base32 secret for the authenticator app
    let secret_base32 = data_encoding::BASE32_NOPAD.encode(&totp_secret);

    // Generate QR code SVG
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1, // skew
        30,
        Secret::Raw(totp_secret.clone())
            .to_bytes()
            .map_err(|_| anyhow::anyhow!("Invalid TOTP secret"))?,
        Some("Archipelago".to_string()),
        "node".to_string(),
    )
    .context("Failed to create TOTP")?;
    let otpauth_url = totp.get_url();
    let qr_svg = generate_qr_svg(&otpauth_url)?;

    let totp_data = TotpData {
        kek_salt: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, kek_salt),
        mek_nonce: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            mek_nonce_bytes,
        ),
        encrypted_mek: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &encrypted_mek,
        ),
        secret_nonce: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            secret_nonce_bytes,
        ),
        encrypted_secret: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &encrypted_secret,
        ),
        backup_codes: hashed_codes,
        used_steps: Vec::new(),
    };

    // Zeroize sensitive material
    mek.zeroize();
    kek.zeroize();
    totp_secret.zeroize();

    Ok(SetupResult {
        totp_data,
        secret_base32,
        qr_svg,
        backup_codes: plaintext_codes,
    })
}

/// Decrypt the TOTP secret from stored data using the user's password.
/// Returns the raw secret bytes.
pub fn decrypt_secret(data: &TotpData, password: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    let b64 = &base64::engine::general_purpose::STANDARD;

    let kek_salt = b64
        .decode(&data.kek_salt)
        .context("Invalid kek_salt base64")?;
    let mek_nonce = b64
        .decode(&data.mek_nonce)
        .context("Invalid mek_nonce base64")?;
    let encrypted_mek = b64
        .decode(&data.encrypted_mek)
        .context("Invalid encrypted_mek base64")?;
    let secret_nonce = b64
        .decode(&data.secret_nonce)
        .context("Invalid secret_nonce base64")?;
    let encrypted_secret = b64
        .decode(&data.encrypted_secret)
        .context("Invalid encrypted_secret base64")?;

    // Derive KEK
    let mut kek = derive_kek(password, &kek_salt)?;

    // Decrypt MEK
    let mut mek = decrypt_chacha(&kek, &mek_nonce, &encrypted_mek)
        .context("Failed to decrypt MEK — wrong password or corrupt data")?;

    // Decrypt TOTP secret
    let secret = decrypt_chacha(&mek, &secret_nonce, &encrypted_secret)
        .context("Failed to decrypt TOTP secret")?;

    kek.zeroize();
    mek.zeroize();

    Ok(secret)
}

/// Verify a TOTP code against the decrypted secret. Checks ±1 time step window.
pub fn verify_code(secret: &[u8], code: &str, used_steps: &[i64]) -> Result<Option<i64>> {
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_vec(),
        None,
        String::new(),
    )
    .context("Failed to create TOTP verifier")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("System time error")?
        .as_secs();

    // Check current step and ±1
    for offset in [-1i64, 0, 1] {
        let time = (now as i64) + (offset * 30);
        if time < 0 {
            continue;
        }
        let step = time / 30;
        let expected = totp.generate(time as u64);

        // Constant-time comparison
        if constant_time_eq(code.as_bytes(), expected.as_bytes()) {
            // Replay protection
            if used_steps.contains(&step) {
                return Ok(None); // Code already used
            }
            return Ok(Some(step));
        }
    }

    Ok(None) // No match
}

/// Verify a backup code against the stored bcrypt hashes. Returns the index if valid.
pub fn verify_backup_code(hashed_codes: &[String], code: &str) -> Result<Option<usize>> {
    let normalized = code.replace('-', "").to_uppercase();
    for (i, hash) in hashed_codes.iter().enumerate() {
        if bcrypt::verify(&normalized, hash)? {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// Re-encrypt the MEK under a new password (for password change).
pub fn rekey(data: &TotpData, old_password: &str, new_password: &str) -> Result<TotpData> {
    use base64::Engine;
    let b64 = &base64::engine::general_purpose::STANDARD;

    // Decrypt MEK with old password
    let old_kek_salt = b64.decode(&data.kek_salt)?;
    let old_mek_nonce = b64.decode(&data.mek_nonce)?;
    let old_encrypted_mek = b64.decode(&data.encrypted_mek)?;

    let mut old_kek = derive_kek(old_password, &old_kek_salt)?;
    let mut mek = decrypt_chacha(&old_kek, &old_mek_nonce, &old_encrypted_mek)
        .context("Failed to decrypt MEK with old password")?;

    // Re-encrypt MEK with new password
    let mut new_kek_salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut new_kek_salt);
    let mut new_kek = derive_kek(new_password, &new_kek_salt)?;

    let mut new_mek_nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut new_mek_nonce);
    let new_encrypted_mek = encrypt_chacha(&new_kek, &new_mek_nonce, &mek)?;

    old_kek.zeroize();
    new_kek.zeroize();
    mek.zeroize();

    Ok(TotpData {
        kek_salt: b64.encode(new_kek_salt),
        mek_nonce: b64.encode(new_mek_nonce),
        encrypted_mek: b64.encode(&new_encrypted_mek),
        // TOTP secret ciphertext unchanged
        secret_nonce: data.secret_nonce.clone(),
        encrypted_secret: data.encrypted_secret.clone(),
        backup_codes: data.backup_codes.clone(),
        used_steps: data.used_steps.clone(),
    })
}

// --- Internal helpers ---

fn derive_kek(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let params = Params::new(
        ARGON2_M_COST,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|e| anyhow::anyhow!("Invalid Argon2 params: {}", e))?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let mut kek = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut kek)
        .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {}", e))?;
    Ok(kek)
}

fn encrypt_chacha(key: &[u8; 32], nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce);
    cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))
}

fn decrypt_chacha(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    let key: &[u8; 32] = key
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid key length"))?;
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))
}

fn generate_backup_codes() -> Result<(Vec<String>, Vec<String>)> {
    let charset: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // No 0/O/1/I ambiguity
    let mut plaintext = Vec::with_capacity(BACKUP_CODE_COUNT);
    let mut hashed = Vec::with_capacity(BACKUP_CODE_COUNT);

    for _ in 0..BACKUP_CODE_COUNT {
        let mut code = String::with_capacity(BACKUP_CODE_LEN);
        for _ in 0..BACKUP_CODE_LEN {
            // KEY-05 migrates the *entropy source* here and nothing else. The
            // `% charset.len()` selection is finding F-09 / backlog R-12, which
            // 10-CONTEXT.md explicitly defers — and the bias is presently zero
            // anyway, because the charset is 32 characters and 32 divides 256
            // exactly. Substituting `SliceRandom::choose`, changing the charset
            // or adding a uniformity test would be executing R-12. The draw is
            // one byte, far below MIN_GUARDED_LEN, so it is unguarded.
            //
            // `next_u32() as u8` is not an approximation of the previous
            // expression, it is the same operation: `rand 0.8`'s
            // `Distribution<u8> for Standard` — which is what `rand::random::<u8>()`
            // resolves to — is itself `rng.next_u32() as u8`. Only the RNG behind
            // it changed, from a defaulted `thread_rng()` to a named `OsRng`.
            let idx =
                (rand::RngCore::next_u32(&mut rand::rngs::OsRng) as u8 as usize) % charset.len();
            code.push(charset[idx] as char);
        }
        let formatted = format!("{}-{}", &code[..4], &code[4..]);
        let hash = bcrypt::hash(&code, 10)?;
        plaintext.push(formatted);
        hashed.push(hash);
    }

    Ok((plaintext, hashed))
}

fn generate_qr_svg(data: &str) -> Result<String> {
    use qrcode::QrCode;
    let code = QrCode::new(data.as_bytes()).context("Failed to generate QR code")?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .quiet_zone(true)
        .build();
    Ok(svg)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_and_verify() {
        let password = "TestPassword123!";
        let result = setup(password).unwrap();

        // Decrypt and verify a code
        let secret = decrypt_secret(&result.totp_data, password).unwrap();
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret.clone(),
            None,
            String::new(),
        )
        .unwrap();
        let code = totp.generate_current().unwrap();

        let step = verify_code(&secret, &code, &[]).unwrap();
        assert!(step.is_some(), "Valid TOTP code should verify");

        // Replay: same step should be rejected
        let used_step = step.unwrap();
        let step2 = verify_code(&secret, &code, &[used_step]).unwrap();
        assert!(step2.is_none(), "Replayed code should be rejected");
    }

    #[test]
    fn test_wrong_password_fails() {
        let result = setup("CorrectPassword1!").unwrap();
        let err = decrypt_secret(&result.totp_data, "WrongPassword1!");
        assert!(err.is_err(), "Wrong password should fail decryption");
    }

    #[test]
    fn test_rekey() {
        let old_pw = "OldPassword123!";
        let new_pw = "NewPassword456!";
        let result = setup(old_pw).unwrap();

        // Get original secret
        let original_secret = decrypt_secret(&result.totp_data, old_pw).unwrap();

        // Rekey
        let rekeyed = rekey(&result.totp_data, old_pw, new_pw).unwrap();

        // Old password should fail
        assert!(decrypt_secret(&rekeyed, old_pw).is_err());

        // New password should produce same secret
        let new_secret = decrypt_secret(&rekeyed, new_pw).unwrap();
        assert_eq!(original_secret, new_secret);
    }

    #[test]
    fn test_backup_codes() {
        let result = setup("TestPassword123!").unwrap();
        assert_eq!(result.backup_codes.len(), BACKUP_CODE_COUNT);

        // Verify a backup code works
        let code = &result.backup_codes[0];
        let idx = verify_backup_code(&result.totp_data.backup_codes, code).unwrap();
        assert_eq!(idx, Some(0));

        // Invalid code should fail
        let bad = verify_backup_code(&result.totp_data.backup_codes, "ZZZZ-ZZZZ").unwrap();
        assert!(bad.is_none());
    }
}
