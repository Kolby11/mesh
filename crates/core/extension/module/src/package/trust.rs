use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The detached signature file carried by a module source tree.
pub const MODULE_SIGNATURE_FILE: &str = "module.sig";

/// The provenance tier assigned to an installed module.
///
/// The ordering is intentional: a policy minimum accepts that tier and every
/// stronger tier. `Verified` is reserved for records carrying a signature
/// that a configured trusted key verifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustTier {
    Core,
    Verified,
    Community,
    Local,
}

impl Default for TrustTier {
    fn default() -> Self {
        Self::Local
    }
}

impl TrustTier {
    pub fn default_for_module(module_id: &str) -> Self {
        if module_id.starts_with("@mesh/") {
            Self::Core
        } else {
            Self::Local
        }
    }

    pub fn for_source(module_id: &str, is_git: bool) -> Self {
        if module_id.starts_with("@mesh/") {
            Self::Core
        } else if is_git {
            Self::Community
        } else {
            Self::Local
        }
    }

    pub fn requires_signature(self) -> bool {
        matches!(self, Self::Verified)
    }
}

/// A detached signature record carried with lock provenance and `module.sig`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedProvenance {
    pub key_id: String,
    pub algorithm: String,
    /// Standard base64 encoded raw signature bytes.
    pub signature: String,
}

impl SignedProvenance {
    pub fn validate(&self) -> Result<(), String> {
        if self.key_id.trim().is_empty() {
            return Err("signed provenance keyId cannot be empty".into());
        }
        if self.algorithm.trim().is_empty() {
            return Err("signed provenance algorithm cannot be empty".into());
        }
        if self.signature.trim().is_empty() {
            return Err("signed provenance signature cannot be empty".into());
        }
        if decode_base64(&self.signature).is_err() {
            return Err("signed provenance signature must be standard base64".into());
        }
        Ok(())
    }
}

/// A public verification key trusted by the root graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustedKey {
    pub algorithm: String,
    /// PEM encoded public key accepted by the configured verification backend.
    pub public_key: String,
}

impl TrustedKey {
    fn validate(&self, key_id: &str) -> Result<(), String> {
        if key_id.trim().is_empty() {
            return Err("trust key id cannot be empty".into());
        }
        if self.algorithm.trim().is_empty() {
            return Err(format!("trust key {key_id} algorithm cannot be empty"));
        }
        if self.public_key.trim().is_empty() {
            return Err(format!("trust key {key_id} publicKey cannot be empty"));
        }
        Ok(())
    }
}

/// User policy for which provenance tiers may enter the active graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustPolicy {
    #[serde(default)]
    pub minimum: TrustTier,
    /// User-controlled trust anchors. A signature never elevates a module
    /// unless its key id resolves in this map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, TrustedKey>,
}

impl Default for TrustPolicy {
    fn default() -> Self {
        Self {
            minimum: TrustTier::Local,
            keys: BTreeMap::new(),
        }
    }
}

impl TrustPolicy {
    pub fn allows(&self, tier: TrustTier) -> bool {
        tier <= self.minimum
    }

    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn validate(&self) -> Result<(), String> {
        for (key_id, key) in &self.keys {
            key.validate(key_id)?;
        }
        Ok(())
    }

    /// Validate one source candidate before it is placed in the live module
    /// tree or admitted to an activation transaction.
    pub fn validate_candidate(
        &self,
        module_id: &str,
        version: &str,
        digest: &str,
        tier: TrustTier,
        signature: Option<&SignedProvenance>,
    ) -> Result<(), String> {
        let assessment = self.assess(module_id, version, digest, tier, signature);
        if !assessment.signature_valid {
            return Err(assessment
                .error
                .unwrap_or_else(|| "provenance signature verification failed".into()));
        }
        if !self.allows(tier) {
            return Err(format!(
                "module {module_id} has {tier:?} provenance, below the configured {:?} trust minimum",
                self.minimum
            ));
        }
        Ok(())
    }

    /// Assess the complete provenance record before graph activation.
    pub(crate) fn assess(
        &self,
        module_id: &str,
        version: &str,
        digest: &str,
        tier: TrustTier,
        signature: Option<&SignedProvenance>,
    ) -> TrustAssessment {
        let Some(signature) = signature else {
            return if tier.requires_signature() {
                TrustAssessment::rejected(
                    tier,
                    "verified provenance requires a detached signature".into(),
                )
            } else {
                TrustAssessment::accepted(tier)
            };
        };

        let Some(key) = self.keys.get(&signature.key_id) else {
            return TrustAssessment::rejected(
                tier,
                format!(
                    "signature key {} is not trusted by the root graph",
                    signature.key_id
                ),
            );
        };
        if !signature.algorithm.eq_ignore_ascii_case(&key.algorithm) {
            return TrustAssessment::rejected(
                tier,
                format!(
                    "signature algorithm {} does not match trusted key {}",
                    signature.algorithm, key.algorithm
                ),
            );
        }

        let payload = signed_provenance_payload(module_id, version, digest);
        let result = match signature.algorithm.to_ascii_lowercase().as_str() {
            "ed25519" => verify_ed25519(&key.public_key, &payload, &signature.signature),
            algorithm => Err(format!("unsupported signature algorithm {algorithm}")),
        };
        match result {
            Ok(()) => TrustAssessment::accepted(tier),
            Err(error) => TrustAssessment::rejected(tier, error),
        }
    }
}

/// The stable bytes signed by a module publisher.
pub fn signed_provenance_payload(module_id: &str, version: &str, digest: &str) -> Vec<u8> {
    format!("mesh-provenance/v1\n{module_id}\n{version}\n{digest}\n").into_bytes()
}

/// Result of applying a root graph's trust policy to one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrustAssessment {
    pub tier: TrustTier,
    pub signature_valid: bool,
    pub error: Option<String>,
}

impl TrustAssessment {
    pub(crate) fn accepted(tier: TrustTier) -> Self {
        Self {
            tier,
            signature_valid: true,
            error: None,
        }
    }

    pub(crate) fn rejected(tier: TrustTier, error: String) -> Self {
        Self {
            tier,
            signature_valid: false,
            error: Some(error),
        }
    }
}

/// Load an optional detached signature from a module directory.
pub fn load_module_signature(
    root: &Path,
) -> Result<Option<SignedProvenance>, super::ModuleManifestError> {
    let path = root.join(MODULE_SIGNATURE_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(super::ModuleManifestError::Io { path, source }),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(super::ModuleManifestError::Validation(format!(
            "{} must be a regular, non-symlink file",
            path.display()
        )));
    }
    super::validate_regular_file(&path, MODULE_SIGNATURE_FILE)?;
    let content = fs::read_to_string(&path).map_err(|source| super::ModuleManifestError::Io {
        path: path.clone(),
        source,
    })?;
    let signature = serde_json::from_str::<SignedProvenance>(&content).map_err(|source| {
        super::ModuleManifestError::Json {
            path: path.clone(),
            source,
        }
    })?;
    signature.validate().map_err(|message| {
        super::ModuleManifestError::Validation(format!(
            "{} has invalid detached provenance: {message}",
            path.display()
        ))
    })?;
    Ok(Some(signature))
}

fn verify_ed25519(public_key: &str, payload: &[u8], signature: &str) -> Result<(), String> {
    let signature = decode_base64(signature)?;
    if signature.len() != 64 {
        return Err(format!(
            "ed25519 signature has {} bytes; expected 64",
            signature.len()
        ));
    }

    let directory = verification_directory()?;
    let result = (|| {
        let key_path = directory.join("key.pem");
        let payload_path = directory.join("payload");
        let signature_path = directory.join("signature");
        fs::write(&key_path, public_key).map_err(|error| {
            format!("failed to stage trusted public key for verification: {error}")
        })?;
        fs::write(&payload_path, payload)
            .map_err(|error| format!("failed to stage provenance payload: {error}"))?;
        fs::write(&signature_path, signature)
            .map_err(|error| format!("failed to stage provenance signature: {error}"))?;

        let output = Command::new("openssl")
            .args(["pkeyutl", "-verify", "-pubin", "-rawin", "-inkey"])
            .arg(&key_path)
            .args(["-in"])
            .arg(&payload_path)
            .args(["-sigfile"])
            .arg(&signature_path)
            .output()
            .map_err(|error| format!("cannot run openssl for ed25519 verification: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(if detail.is_empty() {
                "ed25519 signature verification failed".into()
            } else {
                format!("ed25519 signature verification failed: {detail}")
            })
        }
    })();
    let _ = fs::remove_dir_all(&directory);
    result
}

fn verification_directory() -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("mesh-provenance-{}-{nonce}", std::process::id()));
    fs::create_dir(&directory)
        .map_err(|error| format!("failed to create signature verification workspace: {error}"))?;
    Ok(directory)
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Err("base64 value has an invalid length".into());
    }
    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            base64_value(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            base64_value(chunk[3])?
        };
        if chunk[2] == b'=' && chunk[3] != b'=' {
            return Err("base64 padding is invalid".into());
        }
        if (chunk[2] == b'=' || chunk[3] == b'=') && chunk != &bytes[bytes.len() - 4..] {
            return Err("base64 padding must occur only at the end".into());
        }
        output.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            output.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            output.push((c << 6) | d);
        }
    }
    Ok(output)
}

fn base64_value(value: u8) -> Result<u8, String> {
    match value {
        b'A'..=b'Z' => Ok(value - b'A'),
        b'a'..=b'z' => Ok(value - b'a' + 26),
        b'0'..=b'9' => Ok(value - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        b'=' => Err("unexpected base64 padding".into()),
        _ => Err("base64 value contains an invalid character".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_trust_policy_accepts_only_the_requested_tier_or_stronger() {
        let policy = TrustPolicy {
            minimum: TrustTier::Verified,
            keys: BTreeMap::new(),
        };

        assert!(policy.allows(TrustTier::Core));
        assert!(policy.allows(TrustTier::Verified));
        assert!(!policy.allows(TrustTier::Community));
        assert!(!policy.allows(TrustTier::Local));
    }

    #[test]
    fn verified_provenance_requires_a_signature_and_trusted_key() {
        let policy = TrustPolicy {
            minimum: TrustTier::Verified,
            keys: BTreeMap::new(),
        };
        let assessment = policy.assess(
            "@me/example",
            "1.0.0",
            "sha256:abc",
            TrustTier::Verified,
            None,
        );
        assert!(!assessment.signature_valid);
        assert!(assessment.error.unwrap().contains("requires"));
    }

    #[test]
    fn signed_payload_is_stable_and_framed() {
        assert_eq!(
            signed_provenance_payload("@me/example", "1.0.0", "sha256:abc"),
            b"mesh-provenance/v1\n@me/example\n1.0.0\nsha256:abc\n"
        );
    }

    #[test]
    fn configured_ed25519_key_accepts_the_matching_provenance_payload() {
        let mut keys = BTreeMap::new();
        keys.insert(
            "release".into(),
            TrustedKey {
                algorithm: "ed25519".into(),
                public_key: "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAEYEaRXq7W+vyZLw5SxnhZylZ349Kig7suzbL+cg5Lv4=\n-----END PUBLIC KEY-----\n".into(),
            },
        );
        let policy = TrustPolicy {
            minimum: TrustTier::Verified,
            keys,
        };
        let signature = SignedProvenance {
            key_id: "release".into(),
            algorithm: "ed25519".into(),
            signature: "4cmj4x5zOntkuyqeN+mocm4BAGpTSIfTa++tK7YFPAlMiHPM+/DrW2QOfQ5OmXArNLT9chbK36LjG885e5f9Cg==".into(),
        };

        let assessment = policy.assess(
            "@me/example",
            "1.0.0",
            "sha256:abc",
            TrustTier::Verified,
            Some(&signature),
        );
        assert_eq!(assessment, TrustAssessment::accepted(TrustTier::Verified));
    }

    #[test]
    fn signature_validation_rejects_non_base64() {
        let signature = SignedProvenance {
            key_id: "release".into(),
            algorithm: "ed25519".into(),
            signature: "not base64".into(),
        };
        assert!(signature.validate().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn module_signature_rejects_symlink_before_reading_provenance() {
        let root = verification_directory().unwrap();
        let outside = root
            .parent()
            .unwrap()
            .join(format!("mesh-provenance-outside-{}", std::process::id()));
        fs::write(&outside, "not provenance").unwrap();
        std::os::unix::fs::symlink(&outside, root.join(MODULE_SIGNATURE_FILE)).unwrap();

        let error = load_module_signature(&root).unwrap_err();

        assert!(error.to_string().contains("symlink"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }
}
