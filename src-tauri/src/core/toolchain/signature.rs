//! Verification of detached OpenPGP signatures over checksum files against the
//! pinned keys. `pgp` checks the cryptography; the policy (bindings, signing
//! capability, revocation, expiry, hash strength) is enforced here.

use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use pgp::crypto::hash::HashAlgorithm;
use pgp::packet::{Signature, SignatureType};
use pgp::types::{KeyDetails, VerifyingKey};

use super::ToolchainError;
use super::keys::PinnedKey;

/// Clock skew tolerated for signatures "from the future".
const MAX_FUTURE_SKEW_SECS: u64 = 24 * 3600;

/// A parsed pinned key.
pub struct Keyring {
    keys: Vec<(PinnedKey, SignedPublicKey)>,
}

impl Keyring {
    /// Parses pinned keys; fails if a file does not contain the declared key.
    pub fn load(pinned: &[PinnedKey]) -> Result<Self, ToolchainError> {
        let mut keys = Vec::new();
        for pk in pinned {
            let (key, _) = SignedPublicKey::from_string(pk.armored)
                .map_err(|e| ToolchainError::Key(format!("{}: {e}", pk.owner)))?;
            let fpr = format!("{:X}", key.fingerprint());
            if fpr != pk.fingerprint {
                return Err(ToolchainError::Key(format!(
                    "{}: file contains {fpr}, expected {}",
                    pk.owner, pk.fingerprint
                )));
            }
            key.verify_bindings().map_err(|e| {
                ToolchainError::Key(format!("{}: invalid self-signature: {e}", pk.owner))
            })?;
            keys.push((*pk, key));
        }
        Ok(Self { keys })
    }

    /// Verifies `signature` (binary or ASCII-armored detached signature) over `data`.
    /// Returns the fingerprint of the (sub)key that made it.
    pub fn verify(
        &self,
        data: &[u8],
        signature: &[u8],
        now_unix: u64,
    ) -> Result<String, ToolchainError> {
        let sig = parse_signature(signature)?;
        let s = &sig.signature;
        if !matches!(s.typ(), Some(SignatureType::Binary | SignatureType::Text)) {
            return Err(bad("not a data signature"));
        }
        if matches!(
            s.hash_alg(),
            Some(HashAlgorithm::Md5 | HashAlgorithm::Sha1 | HashAlgorithm::Ripemd160) | None
        ) {
            return Err(bad("weak or missing hash algorithm"));
        }
        let created = s
            .created()
            .map(|t| u64::from(t.as_secs()))
            .ok_or_else(|| bad("signature has no creation time"))?;
        if created > now_unix + MAX_FUTURE_SKEW_SECS {
            return Err(bad("signature creation time is in the future"));
        }

        for (_, key) in &self.keys {
            if !key.details.revocation_signatures.is_empty() {
                continue; // revoked primary key
            }
            let primary_expiry = primary_expiry(key);
            if let Some(exp) = primary_expiry
                && created >= exp
            {
                continue;
            }
            // primary key, if it may sign
            if primary_can_sign(key)
                && issued_by(s, &key.primary_key)
                && sig.verify(key, data).is_ok()
            {
                return Ok(format!("{:X}", key.fingerprint()));
            }
            for sub in &key.public_subkeys {
                if !issued_by(s, &sub.key) || sub.verify_bindings(&key.primary_key).is_err() {
                    continue;
                }
                if sub
                    .signatures
                    .iter()
                    .any(|b| b.typ() == Some(SignatureType::SubkeyRevocation))
                {
                    continue;
                }
                let Some(binding) = latest(&sub.signatures, SignatureType::SubkeyBinding) else {
                    continue;
                };
                if !binding.key_flags().sign() {
                    continue;
                }
                let sub_created = u64::from(sub.key.created_at().as_secs());
                if let Some(d) = binding.key_expiration_time()
                    && created >= sub_created + u64::from(d.as_secs())
                {
                    continue;
                }
                if sig.verify(sub, data).is_ok() {
                    return Ok(format!("{:X}", sub.key.fingerprint()));
                }
            }
        }
        Err(bad("no valid signature from a pinned key"))
    }

    /// Earliest time (unix seconds) at which a pinned key loses its ability to sign
    /// (primary expiry, or expiry of its last signing subkey). `None` = never.
    pub fn signing_expiries(&self) -> Vec<(&'static str, Option<u64>)> {
        self.keys
            .iter()
            .map(|(pk, key)| {
                let primary = primary_expiry(key);
                let mut capable: Vec<Option<u64>> = Vec::new();
                if primary_can_sign(key) {
                    capable.push(primary);
                }
                for sub in &key.public_subkeys {
                    if let Some(b) = latest(&sub.signatures, SignatureType::SubkeyBinding)
                        && b.key_flags().sign()
                    {
                        let created = u64::from(sub.key.created_at().as_secs());
                        let sub_exp = b
                            .key_expiration_time()
                            .map(|d| created + u64::from(d.as_secs()));
                        capable.push(match (sub_exp, primary) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (a, b) => a.or(b),
                        });
                    }
                }
                // the key can sign until its last capable component expires
                let until = if capable.iter().any(Option::is_none) {
                    None
                } else {
                    capable.into_iter().flatten().max()
                };
                (pk.fingerprint, until)
            })
            .collect()
    }
}

fn parse_signature(bytes: &[u8]) -> Result<DetachedSignature, ToolchainError> {
    let text = std::str::from_utf8(bytes).ok();
    let parsed = match text {
        Some(t) if t.trim_start().starts_with("-----BEGIN PGP SIGNATURE-----") => {
            DetachedSignature::from_string(t).map(|(s, _)| s)
        }
        _ => DetachedSignature::from_bytes(bytes),
    };
    parsed.map_err(|e| bad(&format!("unreadable signature: {e}")))
}

fn bad(reason: &str) -> ToolchainError {
    ToolchainError::Signature(reason.to_owned())
}

fn issued_by(sig: &Signature, key: &impl KeyDetails) -> bool {
    let fpr = key.fingerprint();
    let id = key.legacy_key_id();
    sig.issuer_fingerprint().into_iter().any(|f| *f == fpr)
        || sig.issuer_key_id().into_iter().any(|k| *k == id)
}

fn latest(sigs: &[Signature], typ: SignatureType) -> Option<&Signature> {
    sigs.iter()
        .filter(|s| s.typ() == Some(typ))
        .max_by_key(|s| s.created().map(|t| t.as_secs()))
}

/// Latest self-certification of the primary key (over any user id).
fn primary_self_sig(key: &SignedPublicKey) -> Option<&Signature> {
    key.details
        .users
        .iter()
        .flat_map(|u| u.signatures.iter())
        .filter(|s| issued_by(s, &key.primary_key))
        .max_by_key(|s| s.created().map(|t| t.as_secs()))
}

fn primary_expiry(key: &SignedPublicKey) -> Option<u64> {
    let created = u64::from(key.primary_key.created_at().as_secs());
    primary_self_sig(key)
        .and_then(Signature::key_expiration_time)
        .map(|d| created + u64::from(d.as_secs()))
}

fn primary_can_sign(key: &SignedPublicKey) -> bool {
    primary_self_sig(key).is_some_and(|s| s.key_flags().sign())
}

#[allow(dead_code)]
fn _assert_verifying_key<T: VerifyingKey>() {}

#[cfg(test)]
mod tests {
    use super::super::keys;
    use super::*;

    const NOW: u64 = 1_793_000_000; // 2026-10-26

    fn td(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/src/core/toolchain/testdata/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn all_pinned_keys_load_and_match_their_fingerprints() {
        for set in [keys::HASHICORP, keys::OPENTOFU, keys::NODE] {
            Keyring::load(set).unwrap();
        }
    }

    #[test]
    fn no_pinned_key_expires_within_90_days() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let horizon = now + 90 * 24 * 3600;
        for set in [keys::HASHICORP, keys::OPENTOFU, keys::NODE] {
            for (fpr, until) in Keyring::load(set).unwrap().signing_expiries() {
                if let Some(until) = until {
                    assert!(
                        until > horizon,
                        "signing key {fpr} expires within 90 days (at unix {until}); \
                         rotate it, see keys/README.md"
                    );
                }
            }
        }
    }

    #[test]
    fn verifies_real_release_signatures() {
        let hc = Keyring::load(keys::HASHICORP).unwrap();
        let by = hc
            .verify(
                &td("terraform_1.16.6_SHA256SUMS"),
                &td("terraform_1.16.6_SHA256SUMS.72D7468F.sig"),
                NOW,
            )
            .unwrap();
        assert_eq!(by, "374EC75B485913604A831CC7C820C6D5CD27AB87"); // signing subkey

        let tofu = Keyring::load(keys::OPENTOFU).unwrap();
        tofu.verify(
            &td("tofu_1.13.1_SHA256SUMS"),
            &td("tofu_1.13.1_SHA256SUMS.gpgsig"),
            NOW,
        )
        .unwrap();

        let node = Keyring::load(keys::NODE).unwrap();
        let by = node
            .verify(
                &td("node-v24.22.0-SHASUMS256.txt"),
                &td("node-v24.22.0-SHASUMS256.txt.sig"),
                NOW,
            )
            .unwrap();
        assert_eq!(by, "C82FA3AE1CBEDC6BE46B9360C43CEC45C17AB93C");
    }

    #[test]
    fn rejects_tampered_data_wrong_key_and_garbage() {
        let hc = Keyring::load(keys::HASHICORP).unwrap();
        let mut data = td("terraform_1.16.6_SHA256SUMS");
        data[0] ^= 0x01;
        assert!(matches!(
            hc.verify(&data, &td("terraform_1.16.6_SHA256SUMS.72D7468F.sig"), NOW),
            Err(ToolchainError::Signature(_))
        ));
        // OpenTofu signature checked against the HashiCorp key
        assert!(
            hc.verify(
                &td("tofu_1.13.1_SHA256SUMS"),
                &td("tofu_1.13.1_SHA256SUMS.gpgsig"),
                NOW
            )
            .is_err()
        );
        assert!(hc.verify(b"x", b"not a signature", NOW).is_err());
        assert!(hc.verify(b"x", b"", NOW).is_err());
    }

    #[test]
    fn rejects_signatures_from_the_future() {
        let hc = Keyring::load(keys::HASHICORP).unwrap();
        let long_ago = 1_600_000_000; // 2020, before the signature was made
        assert!(
            hc.verify(
                &td("terraform_1.16.6_SHA256SUMS"),
                &td("terraform_1.16.6_SHA256SUMS.72D7468F.sig"),
                long_ago
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_key_file_with_unexpected_fingerprint() {
        let wrong = PinnedKey {
            owner: "test",
            fingerprint: "0000000000000000000000000000000000000000",
            armored: keys::OPENTOFU[0].armored,
        };
        assert!(matches!(
            Keyring::load(&[wrong]),
            Err(ToolchainError::Key(_))
        ));
    }
}
