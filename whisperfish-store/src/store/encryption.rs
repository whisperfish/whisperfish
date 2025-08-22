use anyhow::Context;
use secrecy::ExposeSecret;

const AES_BLOCK_SIZE: usize = 16;

// XXX This crypto module should use libsodium in the future!
/// Functions to encrypt and decrypt storage files
///
/// This module collects cryptography functions that are tied to the storage module. Internally the
/// `secrecy` crate is used to prevent any access to keys' memory. Please look at the documentation
/// of the `block_modes` crate for more information
/// (<https://docs.rs/block-modes/0.8.1/block_modes/trait.BlockMode.html#method.encrypt>) and to the
/// corresponding RFC (<https://datatracker.ietf.org/doc/html/rfc5652#section-6.3>).
#[derive(Debug, Clone)]
pub struct StorageEncryption {
    /// Key for local files
    key_storage: secrecy::Secret<[u8; 16 + 20]>,
    /// Key for the sqlite database
    key_database: secrecy::Secret<[u8; 32]>,
}

impl StorageEncryption {
    /// Derives a storage and a database key. The database key is used to en-/decrypt the database,
    /// the storage key is used to en-/decrypt files on the local hard drive.
    // XXX Is the use of threadpools necessary?
    pub async fn new(
        password: String,
        salt_storage: [u8; 8],
        salt_database: [u8; 8],
    ) -> Result<Self, anyhow::Error> {
        tokio::task::spawn_blocking(move || -> Result<Self, anyhow::Error> {
            let password = password.as_bytes();

            // Derive storage key
            let mut key_storage = [0u8; 16 + 20];
            // Please don't blame me, I'm only the implementer.
            pbkdf2::pbkdf2::<hmac::Hmac<sha1::Sha1>>(
                password,
                &salt_storage,
                1024,
                &mut key_storage,
            )
            .expect("derive pbkdf2 storage key");
            tracing::trace!("Computed the storage key, salt was {:?}", salt_storage);

            // Derive database key
            let params = scrypt::Params::new(14, 8, 1, 32).unwrap();
            let mut key_database = [0u8; 32];
            scrypt::scrypt(password, &salt_database, &params, &mut key_database)
                .context("Cannot compute database key")?;
            tracing::trace!("Computed the database key, salt was {:?}", salt_database);

            // Create self and return
            Ok(Self {
                key_storage: secrecy::Secret::new(key_storage),
                key_database: secrecy::Secret::new(key_database),
            })
        })
        .await
        .map_err(anyhow::Error::from)?
    }

    /// Encrypt data in place. Uses the storage key. IV and MAC are appended to the msg vector.
    pub fn encrypt(&self, msg: &mut Vec<u8>) {
        // Load traits
        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        use hmac::Mac;
        use rand::RngCore;
        type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

        // Generate random IV
        let mut iv = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut iv);

        // Encrypt
        //
        // Create cipher object
        let key = &self.key_storage.expose_secret()[0..16];
        let cipher = Aes128CbcEnc::new(key.into(), &iv.into());

        // The encrypt function expects a vector with the message and appropriate space for
        // padding. Padding is always necessary even if message length is a multiple of aes block
        // size. In this case, a whole block is added for padding.
        let msg_len = msg.len();
        let padding_len = AES_BLOCK_SIZE - (msg_len % AES_BLOCK_SIZE);
        msg.resize(msg_len + padding_len, 0u8);

        // Encrypt the message
        let encrypted_slice = cipher
            .encrypt_padded_mut::<Pkcs7>(&mut msg[..], msg_len)
            .expect("AES CBC encryption error");

        // To be sure that msg vector really got encrypted we compare the length of the returned
        // slice with the msg vector length. See comment at
        // https://gitlab.com/whisperfish/whisperfish/-/merge_requests/200#note_679089540
        assert_eq!(encrypted_slice.len(), msg.len());

        // Create HMAC SHA256, 32 bytes
        let mac = {
            let mut mac =
                hmac::Hmac::<sha2::Sha256>::new_from_slice(&self.key_storage.expose_secret()[16..])
                    .expect("MAC keylength error");
            mac.update(&iv);
            mac.update(msg);
            mac.finalize().into_bytes()
        };

        // Add MAC and ciphertext / msg to IV and replace the reference of msg with reference to
        // the IV vector. It would be better if IV and MAC are added to the message vector (less
        // moving of memory on the heap), but that's not possible due to backwards compatibility.
        let mut iv = iv.to_vec();
        iv.append(msg);
        iv.append(&mut mac.to_vec());

        *msg = iv;
    }

    /// Decrypts message in place. Expects IV and MAC also in msg vector.
    pub fn decrypt(&self, msg: &mut Vec<u8>) -> Result<(), anyhow::Error> {
        use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
        use hmac::Mac;
        type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
        use cipher::generic_array::GenericArray;

        const MIN_MESSAGE_LEN: usize = 16 + 32; // IV (16) + MSG (0) + MAC (32)
        anyhow::ensure!(
            msg.len() >= MIN_MESSAGE_LEN,
            "Attempt at decrypting a message with length {} smaller than minimum length {}",
            msg.len(),
            MIN_MESSAGE_LEN
        );

        // Get IV and MAC from message input vector. We use only slices after here and replace the
        // original message vector in the end.
        let (iv, content) = msg.split_at_mut(16);
        let (content, mac) = content.split_at_mut(content.len() - 32);

        // Verify HMAC SHA256
        let key = &self.key_storage.expose_secret()[0..16];
        let mut verifier =
            hmac::Hmac::<sha2::Sha256>::new_from_slice(&self.key_storage.expose_secret()[16..])
                .expect("MAC keylength error");
        verifier.update(iv);
        verifier.update(content);
        verifier
            .verify(GenericArray::from_slice(mac))
            .map_err(|_| anyhow::anyhow!("MAC verification failed"))?;

        // Decrypt message
        let cipher = Aes128CbcDec::new_from_slices(key, iv).expect("CBC initialization error");
        let cleartext_len = cipher
            .decrypt_padded_mut::<Pkcs7>(content)
            .context("AES CBC decryption error")?
            .len();

        // Remove padding at the end and replace the message reference with a reference to our
        // content variable.
        msg.drain(..16);
        msg.truncate(cleartext_len);

        Ok(())
    }

    /// Return the database key
    pub fn get_database_key(&self) -> &[u8] {
        self.key_database.expose_secret()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check basic encryption and decryption of data.
    #[tokio::test]
    async fn crypto_encrypt_decrypt() {
        let crypto = StorageEncryption::new(String::from("my secret key"), [0u8; 8], [0u8; 8])
            .await
            .unwrap();

        let cleartext = b"my secret message";
        let mut ciphertext = cleartext.to_vec();

        // First, encrypt ciphertext.
        crypto.encrypt(&mut ciphertext);

        // Check here that ciphertext is actually different from cleartext.
        //                                   ⬇ ignore IV
        //                                       ⬇ ignore MAC and use length of cleartext
        // The length of cleartext is used to ignore any possible padding.
        assert_ne!(cleartext[..], ciphertext[16..16 + cleartext.len()]);

        // Then, decrypt ciphertext.
        crypto.decrypt(&mut ciphertext).unwrap();

        // Finally, assert here that cleartext and ciphertext is the same.
        assert_eq!(cleartext, ciphertext.as_slice());
    }

    #[tokio::test]
    async fn crypto_invalid_key() {
        let crypto = StorageEncryption::new(String::from("my secret key"), [0u8; 8], [0u8; 8])
            .await
            .unwrap();
        let cleartext = b"my secret message";
        let mut ciphertext = cleartext.to_vec();
        crypto.encrypt(&mut ciphertext);

        let crypto =
            StorageEncryption::new(String::from("my other secret key"), [0u8; 8], [0u8; 8])
                .await
                .unwrap();

        // Test here whether decryption actually failed because of wrong HMAC
        // We compare both error strings here (message).
        assert_eq!(
            crypto.decrypt(&mut ciphertext).unwrap_err().to_string(),
            anyhow::anyhow!("MAC verification failed").to_string()
        );
    }

    /// In this function we test whether padding of input vector is correct even if length of the
    /// input vector is the same size as aes block size. According to the RFC a whole block is
    /// added if message length is a multiple of aes block size.
    #[tokio::test]
    async fn check_padding_length() {
        let crypto = StorageEncryption::new(String::from("my secret key"), [0u8; 8], [0u8; 8])
            .await
            .unwrap();

        // First, create a cleartext equal to aes block size. We expect padding of one block size.
        let mut cleartext = vec![1u8; AES_BLOCK_SIZE];
        crypto.encrypt(&mut cleartext);

        // Check whether ciphertext is of this length: 2 * aes block size + iv + mac
        assert_eq!(cleartext.len(), 2 * AES_BLOCK_SIZE + 16 + 32);

        // Next, create a cleartext equal to aes block size + 1. We expect a padding of block_size
        // - 1.
        let mut cleartext = vec![1u8; AES_BLOCK_SIZE + 1];
        crypto.encrypt(&mut cleartext);

        // Check whether ciphertext is of this length: 2 * aes block size + iv + mac
        assert_eq!(cleartext.len(), 2 * AES_BLOCK_SIZE + 16 + 32);

        // Next, create a cleartext equal to aes block size - 1. We expect a padding of 1 to the
        // full block size.
        let mut cleartext = vec![1u8; AES_BLOCK_SIZE - 1];
        crypto.encrypt(&mut cleartext);

        // Check whether ciphertext is of this length: aes block size + iv + mac
        assert_eq!(cleartext.len(), AES_BLOCK_SIZE + 16 + 32);
    }

    /// Encrypt with the previous implementation and decrypt with the current implementation ->
    /// Test the backwards compatibility of the current implementation.
    #[tokio::test]
    async fn previous_implementation_encrypt_current_implementation_decrypt() {
        // Our common key and our common salt for the key and cleartext
        let my_key = String::from("my secret key");
        let my_salt = [0u8; 8];
        let my_cleartext = b"my secret message";
        let mut my_ciphertext = std::vec::Vec::new();

        // First, encrypt with the previous implementation
        //
        // Create key
        let mut key = [0u8; 16 + 20];
        assert!(pbkdf2::pbkdf2::<hmac::Hmac<sha1::Sha1>>(
            my_key.as_bytes(),
            &my_salt,
            1024,
            &mut key
        )
        .is_ok());

        // Generate random IV
        use rand::RngCore;
        let mut iv = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut iv);

        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        use cipher::generic_array::GenericArray;
        type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

        // Encrypt
        let cipher = Aes128CbcEnc::new(GenericArray::from_mut_slice(&mut key[..16]), &iv.into());
        let ciphertext = cipher.encrypt_padded_vec_mut::<Pkcs7>(my_cleartext);

        let mac = {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;
            // Verify HMAC SHA256, 32 last bytes
            let mut mac = Hmac::<Sha256>::new_from_slice(&key[16..]).expect("MAC keylength error");
            mac.update(&iv);
            mac.update(&ciphertext);
            mac.finalize().into_bytes()
        };

        // Add iv, ciphertext, mac to `my_ciphertext` vector
        my_ciphertext.append(&mut iv.to_vec());
        my_ciphertext.append(&mut ciphertext.to_vec());
        my_ciphertext.append(&mut mac.to_vec());

        // Now, use the current implementation
        let crypto = StorageEncryption::new(my_key, my_salt, my_salt)
            .await
            .unwrap();

        // Decrypt the ciphertext of the old implementation
        crypto.decrypt(&mut my_ciphertext).unwrap();

        // Assert equalness between cleartext and ciphertext
        assert_eq!(my_cleartext, my_ciphertext.as_slice());
    }

    #[tokio::test]
    async fn decrypt_too_short() {
        let crypto = StorageEncryption::new("lol".into(), [0u8; 8], [0u8; 8])
            .await
            .unwrap();
        let mut ct = vec![0u8; 16 + 31];
        let decr = crypto.decrypt(&mut ct);
        assert!(matches!(decr, Err(anyhow::Error { .. })));
        let err = decr.unwrap_err();
        assert!(err
            .to_string()
            .contains("Attempt at decrypting a message with length 47 smaller"));
    }

    #[tokio::test]
    async fn storage_encryption_clone() {
        let crypto = StorageEncryption::new("lol".into(), [3u8; 8], [7u8; 8])
            .await
            .unwrap();
        let crypto_clone = crypto.clone();
        assert_eq!(
            crypto.key_database.expose_secret(),
            crypto_clone.key_database.expose_secret()
        );
        assert_eq!(
            crypto.key_storage.expose_secret(),
            crypto_clone.key_storage.expose_secret()
        );
        drop(crypto_clone);
    }
}
