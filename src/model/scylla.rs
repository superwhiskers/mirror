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
    cql_to_rust::{FromRow, FromRowError},
    result::Row,
};

/// The type the response is parsed into when a mirroring task queries a mirror channel table
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncompleteMirrorChannelTableRow {
    /// The current offset of the service channel with regard to the mirror channel
    pub stream_offset: u16, // any other type would be overkill for this purpose. the stream itself is limited to 10k (but this could be increased)

                            //TODO: this should be added later or something
                            //      it could be parameterized on the type or something to match the service, to make this same code usable for other nodes
                            // service_options: SomeType,
}

impl FromRow for IncompleteMirrorChannelTableRow {
    fn from_row(row: Row) -> Result<Self, FromRowError> {
        let (stream_offset,) = row.into_typed::<(i16,)>()?;
        Ok(Self {
            stream_offset: stream_offset as u16,
        })
    }
}
