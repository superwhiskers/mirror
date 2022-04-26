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

use fixed_map::{Key, Map};
use once_cell::sync::Lazy;
use scylla::prepared_statement::PreparedStatement;

pub type PreparedStatements = Map<PreparedStatementKey, PreparedStatement>;

pub static PREPARED_STATEMENTS: Lazy<Map<PreparedStatementKey, &str>> = Lazy::new(|| {
    let mut map = Map::new();

    map.insert(
        PreparedStatementKey::GetMirrorChannelLink,
        "SELECT stream_offset FROM linked_mirror_channels WHERE service = ? AND mirror_channel = ? AND service_channel = ?",
    );
    map.insert(
        PreparedStatementKey::GetUserRoles,
        "SELECT role, scope FROM roles WHERE mirror_channel = ? AND user = ?",
    );
    map.insert(
        PreparedStatementKey::GetUserRolesInScope,
        "SELECT role FROM roles WHERE mirror_channel = ? AND user = ? AND scope = ?",
    );
    map.insert(
        PreparedStatementKey::IsUserBanned,
        "SELECT COUNT(*) FROM roles WHERE mirror_channel = ? AND user = ? AND role = 'banned'",
    );
    map.insert(
        PreparedStatementKey::IsUserBannedInScope,
        "SELECT COUNT(*) FROM roles WHERE mirror_channel = ? AND user = ? AND scope = ? AND role = 'banned'",
    );
    map.insert(
        PreparedStatementKey::FetchUsernameFromUsercacheByUserAndService,
        "SELECT username, avatar FROM usercache WHERE user = ? AND service = ?",
    );
    map.insert(
        PreparedStatementKey::InsertMirrorChannelLink,
        "INSERT INTO linked_mirror_channels (service, mirror_channel, service_channel, stream_offset) VALUES (?, ?, ?, ?)",
    );
    map.insert(
        PreparedStatementKey::UpdateMirrorChannelLinkOffset,
        "UPDATE linked_mirror_channels SET stream_offset = ? WHERE service = ? AND mirror_channel = ? AND service_channel = ?",
    );

    map
});

#[derive(Key, Copy, Clone, Debug, Eq, PartialEq)]
pub enum PreparedStatementKey {
    GetMirrorChannelLink,
    GetUserRoles,
    GetUserRolesInScope,
    IsUserBanned,
    IsUserBannedInScope,
    FetchUsernameFromUsercacheByUserAndService,
    InsertMirrorChannelLink,
    UpdateMirrorChannelLinkOffset,
}
