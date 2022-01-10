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

use anyhow::Result;
use std::sync::Arc;
use twilight_http::Client;
use twilight_model::application::{
    callback::{CallbackData, InteractionResponse},
    command::{CommandOption, CommandType},
    interaction::application_command::ApplicationCommand,
};

pub struct Command<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub options: &'a [CommandOption],
    pub default_permission: bool,
    pub kind: CommandType,
}

pub async fn about_handler(
    client: Arc<Client>,
    interaction: Box<ApplicationCommand>,
) -> Result<()> {
    client
        .interaction_callback(
            interaction.id,
            &interaction.token,
            &InteractionResponse::ChannelMessageWithSource(CallbackData {
                allowed_mentions: None,
                components: None,
                content: Some("hello!".to_string()),
                embeds: None,
                flags: None,
                tts: None,
            }),
        )
        .exec()
        .await?;
    Ok(())
}

pub const COMMANDS: [Command; 1] = [Command {
    name: "about",
    description: "display some information about the bot",
    options: &[],
    default_permission: true,
    kind: CommandType::ChatInput,
}];
