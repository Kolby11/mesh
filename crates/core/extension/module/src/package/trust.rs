use serde::{Deserialize, Serialize};

/// The provenance tier assigned to an installed module.
///
/// The ordering is intentional: a policy minimum accepts that tier and every
/// stronger tier. `Verified` is reserved for records carrying a signature;
/// signature cryptographic verification is supplied by package-source
/// integration and is not performed by this metadata type.
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

/// A detached signature record carried with lock provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedProvenance {
    pub key_id: String,
    pub algorithm: String,
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
        Ok(())
    }
}

/// User policy for which provenance tiers may enter the active graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustPolicy {
    #[serde(default)]
    pub minimum: TrustTier,
}

impl Default for TrustPolicy {
    fn default() -> Self {
        Self {
            minimum: TrustTier::Local,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_trust_policy_accepts_only_the_requested_tier_or_stronger() {
        let policy = TrustPolicy {
            minimum: TrustTier::Verified,
        };

        assert!(policy.allows(TrustTier::Core));
        assert!(policy.allows(TrustTier::Verified));
        assert!(!policy.allows(TrustTier::Community));
        assert!(!policy.allows(TrustTier::Local));
    }

    #[test]
    fn verified_lock_provenance_requires_a_signature_record() {
        let signature = SignedProvenance {
            key_id: "mesh-release".into(),
            algorithm: "ed25519".into(),
            signature: "base64-signature".into(),
        };
        assert!(signature.validate().is_ok());
        assert!(TrustTier::Verified.requires_signature());
        assert!(!TrustTier::Community.requires_signature());
    }
}
