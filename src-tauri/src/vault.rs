//! Holds the database key. The key is random, so the PIN can stay short: a copy
//! of the database gives an attacker nothing to guess against. `vault.json`
//! stores the key encrypted with the PIN and then with Windows DPAPI, which
//! only this Windows account can undo. The recovery key is the database key
//! itself, so it still works when `vault.json` or the Windows account is gone.

use crate::db;
use crate::error::{AppError, Result, invalid};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;
use std::thread::sleep;
use std::time::Duration;
use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use zeroize::Zeroizing;

const KEY_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;
const SALT_LENGTH: usize = 16;
const MIN_PIN_LENGTH: usize = 6;
const MAX_PIN_LENGTH: usize = 32;
/// Wrong PINs before each further one is delayed.
const FREE_ATTEMPTS: u32 = 5;
/// Wrong PINs before only the recovery key is accepted.
const MAX_ATTEMPTS: u32 = 10;
const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const ARGON2_PASSES: u32 = 3;
/// Base32 without I, O, 0 and 1, which are easily misread on paper.
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// 256 bits at 5 bits per character.
const RECOVERY_LENGTH: usize = 52;
const RECOVERY_GROUP: usize = 4;

pub const DATABASE_FILE: &str = "financewizard.sqlite";
const VAULT_FILE: &str = "vault.json";
const ENCRYPTING_FILE: &str = "financewizard.encrypting.sqlite";

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// No encrypted database yet: a PIN has to be chosen.
    Setup,
    Locked,
    /// The PIN can no longer open the database, only the recovery key can.
    Recovery,
    Unlocked,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultFile {
    salt: String,
    sealed: String,
    failed_attempts: u32,
}

pub struct Key(Zeroizing<[u8; KEY_LENGTH]>);

fn random_bytes<const N: usize>() -> Result<Zeroizing<[u8; N]>> {
    let mut bytes = Zeroizing::new([0; N]);
    getrandom::fill(&mut *bytes).map_err(|error| AppError::Invalid(error.to_string()))?;
    Ok(bytes)
}

impl Key {
    fn generate() -> Result<Self> {
        Ok(Key(random_bytes()?))
    }

    /// The form SQLCipher takes a raw key in.
    pub fn sql(&self) -> Zeroizing<String> {
        let mut text = Zeroizing::new(String::with_capacity(KEY_LENGTH * 2 + 3));
        text.push_str("x'");
        for byte in self.0.iter() {
            let _ = write!(text, "{byte:02x}");
        }
        text.push('\'');
        text
    }

    fn recovery_code(&self) -> Zeroizing<String> {
        let mut text = Zeroizing::new(String::with_capacity(RECOVERY_LENGTH * 2));
        let (mut bits, mut pending) = (0u16, 0u32);
        let emit = |value: u16, text: &mut String| {
            if !text.is_empty() && text.len() % (RECOVERY_GROUP + 1) == RECOVERY_GROUP {
                text.push('-');
            }
            text.push(ALPHABET[(value & 31) as usize] as char);
        };
        for byte in self.0.iter() {
            bits = (bits << 8) | u16::from(*byte);
            pending += 8;
            while pending >= 5 {
                pending -= 5;
                emit(bits >> pending, &mut text);
            }
            bits &= (1 << pending) - 1;
        }
        emit(bits << (5 - pending), &mut text);
        text
    }

    fn from_recovery_code(code: &str) -> Result<Self> {
        let wrong = "That is not a valid recovery key. Check it for typing mistakes.";
        let mut key = Zeroizing::new([0; KEY_LENGTH]);
        let (mut bits, mut pending, mut written, mut length) = (0u16, 0u32, 0, 0);
        for character in code.chars().filter(|c| c.is_ascii_alphanumeric()) {
            let upper = character.to_ascii_uppercase() as u8;
            let Some(value) = ALPHABET.iter().position(|letter| *letter == upper) else {
                return invalid(wrong);
            };
            length += 1;
            if length > RECOVERY_LENGTH {
                return invalid(wrong);
            }
            bits = (bits << 5) | value as u16;
            pending += 5;
            if pending >= 8 {
                pending -= 8;
                if written < KEY_LENGTH {
                    key[written] = (bits >> pending) as u8;
                    written += 1;
                }
                bits &= (1 << pending) - 1;
            }
        }
        if length != RECOVERY_LENGTH {
            return invalid(wrong);
        }
        Ok(Key(key))
    }
}

fn check_pin(pin: &str) -> Result<()> {
    let digits = pin.chars().all(|c| c.is_ascii_digit());
    if !digits || pin.len() < MIN_PIN_LENGTH || pin.len() > MAX_PIN_LENGTH {
        return invalid(format!("The PIN must be {MIN_PIN_LENGTH} to {MAX_PIN_LENGTH} digits."));
    }
    Ok(())
}

fn derive(pin: &str, salt: &[u8]) -> Result<Zeroizing<[u8; KEY_LENGTH]>> {
    let failed = |error: String| AppError::Invalid(format!("Could not process the PIN: {error}"));
    let params = Params::new(ARGON2_MEMORY_KIB, ARGON2_PASSES, 1, Some(KEY_LENGTH))
        .map_err(|error| failed(error.to_string()))?;
    let mut derived = Zeroizing::new([0; KEY_LENGTH]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin.as_bytes(), salt, &mut *derived)
        .map_err(|error| failed(error.to_string()))?;
    Ok(derived)
}

fn call_dpapi(
    data: &[u8],
    call: impl FnOnce(*const CRYPT_INTEGER_BLOB, *mut CRYPT_INTEGER_BLOB) -> i32,
) -> Option<Zeroizing<Vec<u8>>> {
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
    if call(&input, &mut output) == 0 {
        return None;
    }
    // SAFETY: on success DPAPI returns a buffer of `cbData` bytes that the
    // caller owns and must release with LocalFree.
    unsafe {
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize).fill(0);
        LocalFree(output.pbData as HLOCAL);
        Some(Zeroizing::new(result))
    }
}

fn protect(data: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    call_dpapi(data, |input, output| {
        // SAFETY: the blobs outlive the call and the optional arguments may be null.
        unsafe {
            CryptProtectData(
                input,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                output,
            )
        }
    })
    .ok_or_else(|| AppError::Invalid("Windows could not protect the key.".into()))
}

fn unprotect(data: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    call_dpapi(data, |input, output| {
        // SAFETY: as in `protect`.
        unsafe {
            CryptUnprotectData(
                input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                output,
            )
        }
    })
}

fn seal(key: &Key, pin: &str) -> Result<VaultFile> {
    let salt = random_bytes::<SALT_LENGTH>()?;
    let nonce = random_bytes::<NONCE_LENGTH>()?;
    let cipher = ChaCha20Poly1305::new(derive(pin, &*salt)?.as_ref().into());
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&*nonce), key.0.as_slice())
        .map_err(|_| AppError::Invalid("Could not encrypt the key.".into()))?;
    let mut sealed = nonce.to_vec();
    sealed.extend(encrypted);
    Ok(VaultFile { salt: hex::encode(*salt), sealed: hex::encode(protect(&sealed)?.as_slice()), failed_attempts: 0 })
}

fn open_sealed(file: &VaultFile) -> Option<Zeroizing<Vec<u8>>> {
    unprotect(&hex::decode(&file.sealed).ok()?)
}

/// `None` when the PIN is wrong.
fn unseal(file: &VaultFile, pin: &str) -> Result<Option<Key>> {
    let unavailable = || AppError::Invalid("This PIN can no longer be used. Use your recovery key.".into());
    let sealed = open_sealed(file).filter(|sealed| sealed.len() > NONCE_LENGTH).ok_or_else(unavailable)?;
    let salt = hex::decode(&file.salt).map_err(|_| unavailable())?;
    let (nonce, encrypted) = sealed.split_at(NONCE_LENGTH);
    let cipher = ChaCha20Poly1305::new(derive(pin, &salt)?.as_ref().into());
    let Ok(decrypted) = cipher.decrypt(Nonce::from_slice(nonce), encrypted) else {
        return Ok(None);
    };
    let decrypted = Zeroizing::new(decrypted);
    let mut key = Zeroizing::new([0; KEY_LENGTH]);
    if decrypted.len() != KEY_LENGTH {
        return Err(unavailable());
    }
    key.copy_from_slice(&decrypted);
    Ok(Some(Key(key)))
}

fn database_path(directory: &Path) -> PathBuf {
    directory.join(DATABASE_FILE)
}

fn read_vault(directory: &Path) -> Option<VaultFile> {
    serde_json::from_slice(&fs::read(directory.join(VAULT_FILE)).ok()?).ok()
}

fn write_vault(directory: &Path, file: &VaultFile) -> Result<()> {
    let text = serde_json::to_vec_pretty(file).map_err(|error| AppError::Invalid(error.to_string()))?;
    let temporary = directory.join("vault.json.new");
    fs::write(&temporary, text)?;
    fs::rename(temporary, directory.join(VAULT_FILE))?;
    Ok(())
}

pub fn status(directory: &Path) -> Result<Status> {
    let database = database_path(directory);
    if !database.exists() || db::is_plaintext(&database)? {
        return Ok(Status::Setup);
    }
    Ok(match read_vault(directory) {
        Some(file) if file.failed_attempts < MAX_ATTEMPTS && open_sealed(&file).is_some() => Status::Locked,
        _ => Status::Recovery,
    })
}

/// Encrypts the existing database, or starts a new one, and returns it with the
/// recovery key to show once. The original stays in place until its encrypted
/// replacement has been verified, so a crash here only means setup runs again.
pub fn create(directory: &Path, pin: &str) -> Result<(Connection, Key, Zeroizing<String>)> {
    if status(directory)? != Status::Setup {
        return invalid("A PIN has already been set.");
    }
    check_pin(pin)?;
    let key = Key::generate()?;
    let database = database_path(directory);
    let temporary = directory.join(ENCRYPTING_FILE);
    for leftover in [temporary.clone(), db::sidecar(&temporary, "-wal"), db::sidecar(&temporary, "-shm")] {
        if leftover.exists() {
            fs::remove_file(leftover)?;
        }
    }
    if database.exists() {
        db::encrypt_plaintext(&database, &temporary, &key.sql())?;
    }
    db::open(&temporary, &key.sql())?.close().map_err(|(_, error)| error)?;
    write_vault(directory, &seal(&key, pin)?)?;
    fs::rename(&temporary, &database)?;
    let conn = db::open(&database, &key.sql())?;
    let code = key.recovery_code();
    Ok((conn, key, code))
}

fn after_failure(directory: &Path, mut file: VaultFile) -> Result<String> {
    file.failed_attempts += 1;
    write_vault(directory, &file)?;
    if file.failed_attempts > FREE_ATTEMPTS {
        sleep(Duration::from_secs(1 << (file.failed_attempts - FREE_ATTEMPTS)));
    }
    let left = MAX_ATTEMPTS.saturating_sub(file.failed_attempts);
    Ok(match left {
        0 => "Wrong PIN. Use your recovery key to continue.".to_string(),
        1 => "Wrong PIN. 1 attempt left before the recovery key is needed.".to_string(),
        _ => format!("Wrong PIN. {left} attempts left before the recovery key is needed."),
    })
}

fn check_current_pin(directory: &Path, pin: &str) -> Result<(VaultFile, Key)> {
    let Some(file) = read_vault(directory).filter(|file| file.failed_attempts < MAX_ATTEMPTS) else {
        return invalid("Use your recovery key to continue.");
    };
    match unseal(&file, pin)? {
        Some(key) => Ok((file, key)),
        None => invalid(after_failure(directory, file)?),
    }
}

pub fn unlock(directory: &Path, pin: &str) -> Result<(Connection, Key)> {
    let (mut file, key) = check_current_pin(directory, pin)?;
    let conn = db::open(&database_path(directory), &key.sql())?;
    if file.failed_attempts > 0 {
        file.failed_attempts = 0;
        write_vault(directory, &file)?;
    }
    Ok((conn, key))
}

/// Opens the database with the recovery key and sets a new PIN.
pub fn recover(directory: &Path, code: &str, pin: &str) -> Result<(Connection, Key)> {
    check_pin(pin)?;
    let key = Key::from_recovery_code(code)?;
    let conn = match db::open(&database_path(directory), &key.sql()) {
        Err(AppError::WrongKey) => return invalid("That recovery key does not open this database."),
        other => other?,
    };
    write_vault(directory, &seal(&key, pin)?)?;
    Ok((conn, key))
}

/// Only `vault.json` changes: the database keeps its key.
pub fn change_pin(directory: &Path, current: &str, new: &str) -> Result<()> {
    check_pin(new)?;
    let (_, key) = check_current_pin(directory, current)?;
    write_vault(directory, &seal(&key, new)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{TempDir, plaintext_with_wal};

    const PIN: &str = "482913";

    fn set_failed_attempts(directory: &Path, attempts: u32) {
        let mut file = read_vault(directory).unwrap();
        file.failed_attempts = attempts;
        write_vault(directory, &file).unwrap();
    }

    fn add_setting(conn: &Connection) {
        conn.execute("INSERT INTO settings (key, value) VALUES ('currency', 'USD')", []).unwrap();
    }

    fn currency(conn: &Connection) -> String {
        conn.query_row("SELECT value FROM settings WHERE key = 'currency'", [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn the_recovery_code_round_trips_and_tolerates_formatting() {
        let key = Key::generate().unwrap();
        let code = key.recovery_code();
        assert_eq!(code.chars().filter(|c| *c != '-').count(), RECOVERY_LENGTH);
        let typed = code.replace('-', " ").to_lowercase();
        assert_eq!(*Key::from_recovery_code(&typed).unwrap().sql(), *key.sql());
    }

    #[test]
    fn malformed_recovery_codes_are_rejected() {
        let code = Key::generate().unwrap().recovery_code();
        assert!(Key::from_recovery_code(&code[..code.len() - 1]).is_err());
        assert!(Key::from_recovery_code(&format!("{}A", *code)).is_err());
        assert!(Key::from_recovery_code(&code.replacen(|c: char| c != '-', "0", 1)).is_err());
    }

    #[test]
    fn pins_must_be_six_or_more_digits() {
        assert!(check_pin("12345").is_err());
        assert!(check_pin("12a456").is_err());
        assert!(check_pin("123456").is_ok());
    }

    #[test]
    fn a_new_install_is_encrypted_and_unlocks_with_the_pin() {
        let dir = TempDir::new("vault-new");
        assert_eq!(status(&dir.0).unwrap(), Status::Setup);
        let (conn, _, _) = create(&dir.0, PIN).unwrap();
        add_setting(&conn);
        drop(conn);

        assert_eq!(status(&dir.0).unwrap(), Status::Locked);
        assert!(!db::is_plaintext(&dir.0.join(DATABASE_FILE)).unwrap());
        let (conn, _) = unlock(&dir.0, PIN).unwrap();
        assert_eq!(currency(&conn), "USD");
    }

    #[test]
    fn setup_encrypts_an_existing_database_and_refuses_to_run_twice() {
        let dir = TempDir::new("vault-existing");
        drop(plaintext_with_wal(&dir.0.join(DATABASE_FILE)));
        assert_eq!(status(&dir.0).unwrap(), Status::Setup);

        let (conn, _, _) = create(&dir.0, PIN).unwrap();
        assert_eq!(currency(&conn), "USD");
        assert!(!dir.0.join(ENCRYPTING_FILE).exists());
        assert!(create(&dir.0, PIN).is_err());
    }

    #[test]
    fn wrong_pins_count_up_to_the_recovery_key_and_a_right_one_resets_them() {
        let dir = TempDir::new("vault-attempts");
        drop(create(&dir.0, PIN).unwrap());

        assert!(unlock(&dir.0, "000000").is_err());
        assert_eq!(read_vault(&dir.0).unwrap().failed_attempts, 1);
        drop(unlock(&dir.0, PIN).unwrap());
        assert_eq!(read_vault(&dir.0).unwrap().failed_attempts, 0);

        set_failed_attempts(&dir.0, MAX_ATTEMPTS);
        assert_eq!(status(&dir.0).unwrap(), Status::Recovery);
        assert!(unlock(&dir.0, PIN).is_err());
    }

    #[test]
    fn the_recovery_key_works_with_the_vault_lost_or_unreadable() {
        let dir = TempDir::new("vault-recover");
        let (conn, _, code) = create(&dir.0, PIN).unwrap();
        add_setting(&conn);
        drop(conn);

        let mut file = read_vault(&dir.0).unwrap();
        file.sealed = "00".repeat(40);
        write_vault(&dir.0, &file).unwrap();
        assert_eq!(status(&dir.0).unwrap(), Status::Recovery);

        fs::remove_file(dir.0.join(VAULT_FILE)).unwrap();
        assert_eq!(status(&dir.0).unwrap(), Status::Recovery);

        let (conn, _) = recover(&dir.0, &code, "111111").unwrap();
        assert_eq!(currency(&conn), "USD");
        drop(conn);
        assert_eq!(status(&dir.0).unwrap(), Status::Locked);
        drop(unlock(&dir.0, "111111").unwrap());
    }

    #[test]
    fn another_databases_recovery_key_is_rejected() {
        let dir = TempDir::new("vault-other");
        drop(create(&dir.0, PIN).unwrap());
        let other = Key::generate().unwrap().recovery_code();
        assert!(recover(&dir.0, &other, "111111").is_err());
        assert_eq!(status(&dir.0).unwrap(), Status::Locked);
    }

    #[test]
    fn changing_the_pin_leaves_the_database_file_unchanged() {
        let dir = TempDir::new("vault-change");
        drop(create(&dir.0, PIN).unwrap());
        let before = fs::read(dir.0.join(DATABASE_FILE)).unwrap();

        assert!(change_pin(&dir.0, "000000", "222222").is_err());
        change_pin(&dir.0, PIN, "222222").unwrap();

        assert_eq!(fs::read(dir.0.join(DATABASE_FILE)).unwrap(), before);
        assert!(unlock(&dir.0, PIN).is_err());
        drop(unlock(&dir.0, "222222").unwrap());
    }

    #[test]
    fn dpapi_round_trips_and_rejects_tampering() {
        let sealed = protect(b"secret").unwrap();
        assert_eq!(unprotect(&sealed).unwrap().as_slice(), b"secret");
        let mut changed = sealed.to_vec();
        *changed.last_mut().unwrap() ^= 1;
        assert!(unprotect(&changed).is_none());
    }
}
