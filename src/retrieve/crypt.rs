use crate::{HcOpsError, HcOpsResult};
use base64::Engine;
use diesel::SqliteConnection;
use diesel::connection::SimpleConnection;
use std::path::PathBuf;

pub struct Key {
    key: sodoken::SizedLockedArray<{ sodoken::secretbox::XSALSA_KEYBYTES }>,
    salt: [u8; sodoken::argon2::ARGON2_ID_SALTBYTES],
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Key")
            .field("key", &"hidden")
            .field("salt", &self.salt)
            .finish()
    }
}

impl Key {
    pub fn load(key_path: PathBuf, mut passphrase: sodoken::LockedArray) -> HcOpsResult<Self> {
        let key = std::fs::read_to_string(key_path)?;
        let key = base64::prelude::BASE64_URL_SAFE_NO_PAD
            .decode(key)
            .map_err(HcOpsError::other)?;

        let mut salt = [0; sodoken::argon2::ARGON2_ID_SALTBYTES];

        let salt_index = sodoken::secretbox::XSALSA_NONCEBYTES
            + sodoken::secretbox::XSALSA_MACBYTES
            + sodoken::secretbox::XSALSA_KEYBYTES;
        salt.copy_from_slice(&key[salt_index..salt_index + sodoken::argon2::ARGON2_ID_SALTBYTES]);

        let mut secret =
            sodoken::SizedLockedArray::<{ sodoken::secretbox::XSALSA_KEYBYTES }>::new()?;
        sodoken::argon2::blocking_argon2id(
            &mut *secret.lock(),
            &passphrase.lock(),
            &salt,
            sodoken::argon2::ARGON2_ID_OPSLIMIT_MODERATE,
            sodoken::argon2::ARGON2_ID_MEMLIMIT_MODERATE,
        )?;

        let mut nonce = [0; sodoken::secretbox::XSALSA_NONCEBYTES];
        nonce.copy_from_slice(&key[..sodoken::secretbox::XSALSA_NONCEBYTES]);

        let mut cipher =
            [0; sodoken::secretbox::XSALSA_MACBYTES + sodoken::secretbox::XSALSA_KEYBYTES];
        cipher.copy_from_slice(
            &key[sodoken::secretbox::XSALSA_NONCEBYTES
                ..sodoken::secretbox::XSALSA_NONCEBYTES
                    + sodoken::secretbox::XSALSA_MACBYTES
                    + sodoken::secretbox::XSALSA_KEYBYTES],
        );

        let mut key = sodoken::SizedLockedArray::<{ sodoken::secretbox::XSALSA_KEYBYTES }>::new()?;

        sodoken::secretbox::xsalsa_open_easy(&mut *key.lock(), &cipher, &nonce, &secret.lock())
            .map_err(|_| HcOpsError::Other("Failed to decrypt key".into()))?;

        Ok(Key { key, salt })
    }
}

pub fn apply_key(conn: &mut SqliteConnection, key: &mut Key) -> HcOpsResult<()> {
    static PRAGMA: &[u8] = br#"
PRAGMA key = "x'----------------------------------------------------------------'";
PRAGMA cipher_salt = "x'--------------------------------'";
PRAGMA cipher_compatibility = 4;
PRAGMA cipher_plaintext_header_size = 32;
"#;

    let mut stmt = sodoken::LockedArray::new(PRAGMA.len())?;
    stmt.lock().copy_from_slice(PRAGMA);

    {
        let mut lock = stmt.lock();
        for (i, b) in key.key.lock().iter().enumerate() {
            let c = format!("{b:02X}");
            let idx = 17 + (i * 2);
            lock[idx..idx + 2].copy_from_slice(c.as_bytes())
        }
        for (i, b) in key.salt.iter().enumerate() {
            let c = format!("{b:02X}");
            let idx = 109 + (i * 2);
            lock[idx..idx + 2].copy_from_slice(c.as_bytes())
        }
    }

    conn.batch_execute(std::str::from_utf8(&stmt.lock()).map_err(HcOpsError::other)?)?;

    Ok(())
}
