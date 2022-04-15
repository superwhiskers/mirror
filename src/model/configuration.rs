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

use config::{Config, ConfigBuilder, ConfigError, Environment, File};
use serde::Deserialize;
use std::collections::HashSet;

/// An abstract representation of mirror's configuration
#[derive(Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Configuration {
    /// The general section of the configuration
    pub general: GeneralConfiguration,

    /// The rabbitmq message queue section of the configuration
    #[serde(default)]
    pub rabbitmq: RabbitmqConfiguration,

    /// The scylla database section of the configuration
    #[serde(default)]
    pub scylla: ScyllaConfiguration,
}

impl Configuration {
    pub fn new() -> Result<Self, ConfigError> {
        //TODO(superwhiskers): i'm not sure if this actually works as i expect it to
        Config::builder()
            .add_source(File::with_name("config.toml").required(false))
            .add_source(
                Environment::with_prefix("mirror")
                    .separator("_")
                    .list_separator(",")
                    .with_list_parse_key("rabbitmq.addresses")
                    .with_list_parse_key("scylla.addresses"),
            )
            .build()?
            .try_deserialize()
    }
}

/// A representation of the general section of mirror's configuration
#[derive(Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct GeneralConfiguration {
    /// The bot's token
    pub token: String,

    /// The bot's admins
    //TODO(superwhiskers): consider removing this and having them purely managed through
    //                     scylla
    pub admins: Option<HashSet<u64>>,

    /// The logger configuration
    #[serde(default = "default_logger")]
    pub logger: String,
}

/// The default value of the `logger` field on the [`GeneralConfiguration`]
#[inline(always)]
fn default_logger() -> String {
    "info".to_string()
}

/// A representation of the rabbitmq section of mirror's configuration
#[derive(Deserialize, Clone, Debug, Eq, Hash, PartialEq)]
pub struct RabbitmqConfiguration {
    /// The addresses of different rabbitmq nodes to try connecting to
    #[serde(default = "default_rabbitmq_addresses")]
    pub addresses: Vec<String>,

    /// The maximum number of connections that can be open to a rabbitmq node at any given time
    #[serde(default = "default_rabbitmq_max")]
    pub max: usize,
    //TODO(superwhiskers): add rabbitmq client-certificate based authentication options here
}

impl Default for RabbitmqConfiguration {
    fn default() -> Self {
        Self {
            addresses: default_rabbitmq_addresses(),
            max: default_rabbitmq_max(),
        }
    }
}

/// The default value of the `addresses` field on the [`RabbitmqConfiguration`]
#[inline(always)]
fn default_rabbitmq_addresses() -> Vec<String> {
    vec!["amqp://localhost".to_string()]
}

/// The default value of the `max` field on the [`RabbitmqConfiguration`]
#[inline(always)]
fn default_rabbitmq_max() -> usize {
    20
}

/// A representation of the scylla section of mirror's configuration
#[derive(Deserialize, Clone, Debug, Eq, Hash, PartialEq)]
pub struct ScyllaConfiguration {
    /// The addresses of different scylla nodes to try connecting to
    #[serde(default = "default_scylla_addresses")]
    pub addresses: Vec<String>,

    /// The username to authenticate with when connecting to Scylla
    pub username: Option<String>,

    /// The password to authenticate with when connecting to Scylla
    pub password: Option<String>,
}

impl Default for ScyllaConfiguration {
    fn default() -> Self {
        Self {
            addresses: default_scylla_addresses(),
            username: None,
            password: None,
        }
    }
}

/// The default value of the `addresses` field on the [`ScyllaConfiguration`]
#[inline(always)]
fn default_scylla_addresses() -> Vec<String> {
    vec!["localhost".to_string()]
}
