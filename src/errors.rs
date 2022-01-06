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

use thiserror::Error;
use twilight_gateway::cluster::scheme::ShardScheme;

/// An enumeration over errors that may be raised by the bot and are not well described enough by
/// existing errors
#[derive(Error, Debug)]
pub enum Error {
    /// An error that may arise when the chosen sharding scheme is not recognized by the bot
    #[error("`{0:?}` is not a recognized sharding scheme")]
    UnknownShardingScheme(ShardScheme),
}
