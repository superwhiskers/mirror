//
//  mirror - a fast channel mirroring bot in rust
//  Copyright (C) superwhiskers <whiskerdev@protonmail.com> 2020-2021
//
//  This program is free software: you can redistribute it and/or modify
//  it under the terms of the GNU Affero General Public License as published by
//  the Free Software Foundation, either version 3 of the License, or
//  (at your option) any later version.
//
//  This program is distributed in the hope that it will be useful,
//  but WITHOUT ANY WARRANTY; without even the implied warranty of
//  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//  GNU Affero General Public License for more details.
//
//  You should have received a copy of the GNU Affero General Public License
//  along with this program.  If not, see <https://www.gnu.org/licenses/>.
//

use scylla::frame::response::{
    cql_to_rust::{FromCqlVal, FromCqlValError},
    result::CqlValue,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    fmt::{self, Display},
    num::{NonZeroU64, ParseIntError},
    str::FromStr,
};
use thiserror::Error;
use twilight_model::id::{marker::GenericMarker, Id};

/// The service associated with an [`Identifier`]
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum IdentifierKind {
    Discord,
    MirrorChannel,
}

impl IdentifierKind {
    pub fn as_str(&self) -> &str {
        match &self {
            Self::Discord => "discord",
            Self::MirrorChannel => "messages",
        }
    }
}

impl Display for IdentifierKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// An identifier stored in mirror's database
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum Identifier<'a> {
    /// An identifier used on the Discord platform
    Discord(Id<GenericMarker>),

    /// An identifier for a mirror channel
    MirrorChannel(Cow<'a, str>),
}

impl<'a> Identifier<'a> {
    pub fn kind(&self) -> IdentifierKind {
        match &self {
            Self::Discord(_) => IdentifierKind::Discord,
            Self::MirrorChannel(_) => IdentifierKind::MirrorChannel,
        }
    }
}

impl<'a> FromCqlVal<CqlValue> for Identifier<'a> {
    fn from_cql(value: CqlValue) -> Result<Self, FromCqlValError> {
        Self::from_str(value.as_text().ok_or(FromCqlValError::BadCqlType)?)
            .map_err(|_| FromCqlValError::BadCqlType)
    }
}

impl<'a> FromStr for Identifier<'a> {
    type Err = ParseIdentifierError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.split_once('-') {
            Some(("discord", id)) => Self::Discord(id.parse()?),
            Some(("messages", id)) => Self::MirrorChannel(Cow::Owned(id.to_string())),
            Some((service, _)) => {
                // i personally don't like the allocation here--i think it should be removed at
                // some point or another, but there is no way to add lifetime parameters to the
                // [`FromStr`] trait without editing std
                return Err(ParseIdentifierError::InvalidService(service.to_string()));
            }
            None => return Err(ParseIdentifierError::InvalidStructure),
        })
    }
}

impl<'a> Display for Identifier<'a> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Self::Discord(id) => write!(formatter, "discord-{}", id),
            Self::MirrorChannel(name) => write!(formatter, "messages-{}", name),
        }
    }
}

/// An enumeration over errors that may arise when parsing a string into an [`Identifier`]
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum ParseIdentifierError {
    /// An error encountered when an integer could not be parsed
    #[error("An error was encountered while trying to parse a string as an integer")]
    IntegerParseError(#[from] ParseIntError),

    /// An error encountered when the service portion of the identifier (the part of the string
    /// before the first dash) is not a known service
    #[error("`{0}` is not a recognized service")]
    InvalidService(String),

    /// An error encountered when the structure of the identifier's string representation is
    /// invalid (such as when there is no `-`)
    #[error("The identifier's structure was invalid")]
    InvalidStructure,
}
