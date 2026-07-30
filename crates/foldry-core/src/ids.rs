use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, de::Error as DeError};
use uuid::Uuid;

/// Failure to construct one of Foldry's UUIDv7 identifiers.
#[derive(Debug)]
pub enum IdParseError {
    InvalidUuid(uuid::Error),
    NotVersion7,
}

impl fmt::Display for IdParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUuid(error) => write!(formatter, "invalid UUID: {error}"),
            Self::NotVersion7 => formatter.write_str("identifier must be a UUIDv7"),
        }
    }
}

impl std::error::Error for IdParseError {}

macro_rules! uuid_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a time-ordered UUIDv7 identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Validates and wraps an existing UUIDv7 value.
            pub fn from_uuid(value: Uuid) -> Result<Self, IdParseError> {
                if value.get_version_num() == 7 {
                    Ok(Self(value))
                } else {
                    Err(IdParseError::NotVersion7)
                }
            }

            /// Returns the underlying UUID value.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let uuid = Uuid::parse_str(value).map_err(IdParseError::InvalidUuid)?;
                Self::from_uuid(uuid)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(D::Error::custom)
            }
        }
    };
}

uuid_id!(ProfileId, "Stable identity of a filtering profile.");
uuid_id!(FolderId, "Stable identity of a configured folder.");
uuid_id!(ActionId, "Stable identity of one configured folder action.");
uuid_id!(RunId, "Stable identity of one action execution.");

/// Stable lowercase key of a reusable profile preset.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PresetId(String);

impl PresetId {
    pub const MAX_LENGTH: usize = 64;

    /// Returns the validated wire value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PresetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for PresetId {
    type Err = PresetIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = !value.is_empty()
            && value.len() <= Self::MAX_LENGTH
            && value.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
            })
            && !value.ends_with('-')
            && !value.contains("--");
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(PresetIdParseError)
        }
    }
}

impl<'de> Deserialize<'de> for PresetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

/// Invalid human-readable preset identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetIdParseError;

impl fmt::Display for PresetIdParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "preset id must be a lowercase ASCII slug of 1-64 letters, digits, or single hyphens",
        )
    }
}

impl std::error::Error for PresetIdParseError {}

#[cfg(test)]
mod tests {
    use super::{ActionId, FolderId, PresetId, ProfileId, RunId};

    #[test]
    fn generated_identifiers_are_uuid_v7() {
        assert_eq!(ProfileId::new().as_uuid().get_version_num(), 7);
        assert_eq!(FolderId::new().as_uuid().get_version_num(), 7);
        assert_eq!(ActionId::new().as_uuid().get_version_num(), 7);
        assert_eq!(RunId::new().as_uuid().get_version_num(), 7);
    }

    #[test]
    fn identifiers_round_trip_through_strings() {
        let original = ProfileId::new();
        let parsed = original.to_string().parse::<ProfileId>().unwrap();

        assert_eq!(parsed, original);
    }

    #[test]
    fn non_v7_identifiers_are_rejected() {
        let error = "550e8400-e29b-41d4-a716-446655440000"
            .parse::<ProfileId>()
            .unwrap_err();

        assert_eq!(error.to_string(), "identifier must be a UUIDv7");
    }

    #[test]
    fn preset_identifiers_use_stable_human_readable_slugs() {
        assert_eq!(
            "test-artifacts".parse::<PresetId>().unwrap().as_str(),
            "test-artifacts"
        );
        assert!("Test Artifacts".parse::<PresetId>().is_err());
        assert!("test--artifacts".parse::<PresetId>().is_err());
    }
}
