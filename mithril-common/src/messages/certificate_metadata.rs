use crate::entities::{ProtocolParameters, ProtocolVersion, SignerWithStake};
use serde::{Deserialize, Serialize};

/// CertificateMetadata represents the metadata associated to a Certificate
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct CertificateMetadataMessage {
    /// Protocol Version (semver)
    /// Useful to achieve backward compatibility of the certificates (including of the multi signature)
    /// part of METADATA(p,n)
    #[serde(rename = "version")]
    pub protocol_version: ProtocolVersion,

    /// Protocol parameters
    /// part of METADATA(p,n)
    #[serde(rename = "parameters")]
    pub protocol_parameters: ProtocolParameters,

    /// Date and time when the certificate was initiated
    /// Represents the time at which the single signatures registration is opened
    /// part of METADATA(p,n)
    pub initiated_at: String,

    /// Date and time when the certificate was sealed
    /// Represents the time at which the quorum of single signatures was reached so that they were aggregated into a multi signature
    /// part of METADATA(p,n)
    pub sealed_at: String,

    /// The list of the active signers with their stakes and verification keys
    /// part of METADATA(p,n)
    pub signers: Vec<SignerWithStake>,
}

impl CertificateMetadataMessage {
    /// CertificateMetadata factory
    pub fn dummy() -> Self {
        Self {
            protocol_version: "0.1.0".to_string(),
            protocol_parameters: ProtocolParameters::new(1000, 100, 0.123),
            initiated_at: "initiated_at".to_string(),
            sealed_at: "sealed_at".to_string(),
            signers: vec![
                SignerWithStake::new(
                    "1".to_string(),
                    "verification-key-123".to_string(),
                    None,
                    None,
                    None,
                    10,
                ),
                SignerWithStake::new(
                    "2".to_string(),
                    "verification-key-456".to_string(),
                    None,
                    None,
                    None,
                    20,
                ),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden_message() -> CertificateMetadataMessage {
        CertificateMetadataMessage {
            protocol_version: "0.1.0".to_string(),
            protocol_parameters: ProtocolParameters::new(1000, 100, 0.123),
            initiated_at: "initiated_at".to_string(),
            sealed_at: "sealed_at".to_string(),
            signers: vec![
                SignerWithStake::new(
                    "1".to_string(),
                    "verification-key-123".to_string(),
                    None,
                    None,
                    None,
                    10,
                ),
                SignerWithStake::new(
                    "2".to_string(),
                    "verification-key-456".to_string(),
                    None,
                    None,
                    None,
                    20,
                ),
            ],
        }
    }

    // Test the backward compatibility with possible future upgrades.
    #[test]
    fn test_v1() {
        let json = r#"{
            "version": "0.1.0",
            "parameters": {
                "k": 1000,
                "m": 100,
                "phi_f": 0.123
            },
            "initiated_at": "initiated_at",
            "sealed_at": "sealed_at",
            "signers": [
                {
                    "party_id": "1",
                    "verification_key": "verification-key-123",
                    "stake": 10
                },
                {
                    "party_id": "2",
                    "verification_key": "verification-key-456",
                    "stake": 20
                }
            ]
        }"#;
        let message: CertificateMetadataMessage = serde_json::from_str(json).expect(
            "This JSON is expected to be successfully parsed into a CertificateMetadataMessage instance.",
        );

        assert_eq!(golden_message(), message);
    }
}
