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

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use uuid::Uuid;

use crate::model::identifier::Identifier;

/// An enumeration over the events sent over a node's amqp queue
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum NodeQueueUpdate {
    /// An event that signals to the node the queue belongs to that it should end the task
    /// corresponding to the information in the structure
    EndTask {
        to: Identifier,
        from: Identifier,
        node_id: Uuid,
    },
}

/// An enumeration over the visibility of a mirror channel
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum MirrorChannelVisibility {
    Public,
    // consider adding one for "you're able to join if you have some piece of information linked to it"
    Private,
}

/// An enumeration over mirror channel configuration items
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum MirrorChannelConfigurationItem {
    // we could put channel visibility here but it's not necessary
}

#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum ServiceChannelConfigurationItem {
    // there isn't anything here yet because we don't need offset updates since it's updated by the task this type is for
}

/// An enumeration over the events sent over a mirror channel stream
//TODO(superwhiskers): add in other features and add these back as necessary
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum MirrorChannelStreamUpdate {
    /// An event that signals there has been an update to the mirror channel's configuration
    //MirrorChannelConfiguration(MirrorChannelConfigurationItem),

    /// An event that signals there has been an update to the service channel's configuration
    //ServiceChannelConfiguration(ServiceChannelConfigurationItem),

    /// An event indicating a message has been sent
    Message {
        author: Identifier,
        content: String,
    },
}
