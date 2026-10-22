//! Strongly-typed Discord snowflake identifiers.
//!
//! Every snowflake column is a `BIGINT` in the database and an `i64` in the
//! model, wrapped in a distinct newtype so a guild id can never be assigned
//! where a user id is expected. Discord snowflakes are 63-bit positive values,
//! so the bit-preserving reinterpretation below never loses data.

use std::fmt;

use sea_orm::DeriveValueType;

macro_rules! snowflake {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, DeriveValueType)]
        pub struct $name(
            /// Raw 64-bit representation of the snowflake.
            pub i64,
        );

        impl $name {
            /// Reinterpret a Discord snowflake `u64` as the stored `i64`.
            pub const fn from_raw(raw: u64) -> Self {
                Self(i64::from_ne_bytes(raw.to_ne_bytes()))
            }

            /// Extract the underlying u64 snowflake.
            pub const fn get(self) -> u64 {
                u64::from_ne_bytes(self.0.to_ne_bytes())
            }

            /// Return the underlying stored i64 value.
            pub const fn as_i64(self) -> i64 {
                self.0
            }
        }

        impl From<u64> for $name {
            fn from(raw: u64) -> Self {
                Self::from_raw(raw)
            }
        }

        impl From<i64> for $name {
            fn from(raw: i64) -> Self {
                Self(raw)
            }
        }

        impl From<std::num::NonZeroU64> for $name {
            fn from(raw: std::num::NonZeroU64) -> Self {
                Self::from_raw(raw.get())
            }
        }

        impl From<$name> for u64 {
            fn from(id: $name) -> Self {
                id.get()
            }
        }

        impl From<$name> for i64 {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl std::str::FromStr for $name {
            type Err = std::num::ParseIntError;

            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                let raw: u64 = s.parse()?;
                Ok(Self::from_raw(raw))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.get())
            }
        }
    };
}

snowflake!(
    /// A guild snowflake.
    GuildId
);
snowflake!(
    /// A user snowflake. Also used for actors, moderators and message authors.
    UserId
);
snowflake!(
    /// A channel snowflake.
    ChannelId
);
snowflake!(
    /// A permit assignment target: a user, role or channel depending on
    /// `permit_assignments.target_type`.
    SnowflakeId
);

impl From<GuildId> for SnowflakeId {
    fn from(id: GuildId) -> Self {
        Self(id.0)
    }
}

impl From<UserId> for SnowflakeId {
    fn from(id: UserId) -> Self {
        Self(id.0)
    }
}

impl From<ChannelId> for SnowflakeId {
    fn from(id: ChannelId) -> Self {
        Self(id.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_discord_snowflakes() {
        let raw = 1_234_567_890_123_456_789_u64;
        assert_eq!(GuildId::from_raw(raw).get(), raw);
        assert_eq!(UserId::from(raw).get(), raw);
        assert_eq!(GuildId::from_raw(0).as_i64(), 0);
        assert!(matches!("12345".parse::<UserId>(), Ok(id) if id.get() == 12_345));
        assert_eq!(SnowflakeId::from(UserId::from_raw(42)).as_i64(), 42);
    }
}
