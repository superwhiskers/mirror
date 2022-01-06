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

#![allow(clippy::cognitive_complexity)]
#![warn(clippy::cargo_common_metadata)]
#![warn(clippy::dbg_macro)]
#![warn(clippy::explicit_deref_methods)]
#![warn(clippy::filetype_is_file)]
#![warn(clippy::imprecise_flops)]
#![warn(clippy::large_stack_arrays)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![deny(clippy::await_holding_lock)]
#![deny(clippy::cast_lossless)]
#![deny(clippy::clone_on_ref_ptr)]
#![deny(clippy::doc_markdown)]
#![deny(clippy::empty_enum)]
#![deny(clippy::enum_glob_use)]
#![deny(clippy::exit)]
#![deny(clippy::explicit_into_iter_loop)]
#![deny(clippy::explicit_iter_loop)]
#![deny(clippy::fallible_impl_from)]
#![deny(clippy::inefficient_to_string)]
#![deny(clippy::large_digit_groups)]
#![deny(clippy::wildcard_dependencies)]
#![deny(clippy::wildcard_imports)]
#![deny(clippy::unused_self)]
#![deny(clippy::single_match_else)]
#![deny(clippy::option_option)]
#![deny(clippy::mut_mut)]
#![feature(async_closure)]

use anyhow::{Context, Result};
use futures::{
    future::{self, FutureExt, TryFutureExt},
    stream::StreamExt,
};
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, ExchangeDeclareOptions,
        QueueBindOptions, QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable},
    ExchangeKind,
};
use once_cell::sync::Lazy;
use std::{borrow::Cow, collections::HashSet, future::Future, iter, pin::Pin, str, sync::Arc};
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};
use tracing_log::LogTracer;
use tracing_subscriber::FmtSubscriber;
use twilight_model::{
    application::interaction::Interaction,
    gateway::event::Event,
    id::{ChannelId, GenericId},
};
use uuid::{adapter::Simple as SimpleUuidAdapter, Uuid};

mod commands;
mod errors;
mod init;
mod model;
mod prepared_statements;
mod tasks;

use crate::{
    model::{configuration::Configuration, identifier::Identifier, rabbitmq::NodeQueueUpdate},
    tasks::MirrorTaskSubscriptionUpdate,
};

static NODE_ID_RAW: Lazy<[u8; SimpleUuidAdapter::LENGTH]> = Lazy::new(|| {
    let mut id = [0; SimpleUuidAdapter::LENGTH];

    trace!("generating the node's uuid");

    let uuid = Uuid::new_v4().to_simple();

    uuid.encode_lower(&mut id);

    id
});

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::new().context("failed to load the configuration")?;

    tracing::subscriber::set_global_default(
        FmtSubscriber::builder()
            .with_env_filter(&config.general.logger)
            .finish(),
    )?;

    LogTracer::init()?;

    trace!("starting the bot");

    let node_id_rabbitmq_queue_raw = {
        let mut rabbitmq_stream: [u8; 13 + SimpleUuidAdapter::LENGTH] =
            [0; 13 + SimpleUuidAdapter::LENGTH];

        rabbitmq_stream[0] = b'n';
        rabbitmq_stream[1] = b'o';
        rabbitmq_stream[2] = b'd';
        rabbitmq_stream[3] = b'e';
        rabbitmq_stream[4] = b'.';
        rabbitmq_stream[5] = b'd';
        rabbitmq_stream[6] = b'i';
        rabbitmq_stream[7] = b's';
        rabbitmq_stream[8] = b'c';
        rabbitmq_stream[9] = b'o';
        rabbitmq_stream[10] = b'r';
        rabbitmq_stream[11] = b'd';
        rabbitmq_stream[12] = b'.';

        rabbitmq_stream[13..].copy_from_slice(NODE_ID_RAW.as_slice());

        rabbitmq_stream
    };
    let node_id = str::from_utf8(NODE_ID_RAW.as_slice())?;
    let node_id_rabbitmq_queue = str::from_utf8(&node_id_rabbitmq_queue_raw)?;

    info!("consumer id: {}", node_id);
    debug!("rabbitmq channel: {}", node_id_rabbitmq_queue);

    let rabbitmq = init::rabbitmq(&config).await?;

    //TODO(superwhiskers): remove this
    rabbitmq
        .get()
        .await?
        .create_channel()
        .await?
        .basic_publish(
            "",
            "messages.messages-test",
            lapin::options::BasicPublishOptions::default(),
            rmp_serde::to_vec(&model::rabbitmq::MirrorChannelStreamUpdate::Message {
                author: model::identifier::Identifier::MirrorChannel("test".into()),
                content: "whatever".into(),
            })?,
            lapin::BasicProperties::default(),
        )
        .await?;

    //TODO(superwhiskers): finish handling this
    let (scylla, prepared_statements) = init::scylla(&config).await?;

    let (http_client, _application_info) = init::discord_http(&config).await?;

    init::discord_commands(Arc::clone(&http_client)).await?;

    let (mut cluster, mut events) = init::discord_gateway(&config).await?;

    let (_mirror_manager_handle, mirror_manager_channel) = init::mirroring_tasks(
        node_id,
        rabbitmq.clone(),
        Arc::clone(&scylla),
        Arc::clone(&http_client),
        Arc::clone(&cluster),
        Arc::clone(&prepared_statements),
    )
    .await?;

    //TODO(superwhiskers): somewhere here we need to spawn a rabbitmq connection, pull it out
    //                     of the pool using the `.take()` method, and then subscribe to a
    //                     stream that other nodes can use to request that the currently
    //                     running node stop mirroring to specific service channels and
    //                     listening to specific service channels

    let rabbitmq_channel = rabbitmq.get().await?.create_channel().await?;
    rabbitmq_channel
        .exchange_declare(
            "nodes",
            ExchangeKind::Direct,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;
    let _ = rabbitmq_channel
        .queue_declare(
            node_id_rabbitmq_queue,
            QueueDeclareOptions {
                auto_delete: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;
    rabbitmq_channel
        .queue_bind(
            node_id_rabbitmq_queue,
            "nodes",
            "discord",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    //TODO(superwhiskers): consume from the queue and cancel consuming once we are done with
    //                     it
    let _node_queue_handle = tokio::spawn(
        rabbitmq_channel
            .basic_consume(
                node_id_rabbitmq_queue,
                "",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?
            .for_each_concurrent(None, move |potential_update| {
                match potential_update {
                    Ok((rabbitmq_channel, potential_update)) => {
                        let update = rmp_serde::from_read::<_, NodeQueueUpdate>(potential_update.data.as_slice());
                        trace!("received node queue update event: {:?}", update);

                        match update {
                            Ok(update) => match update {
                                NodeQueueUpdate::EndTask {
                                    to: Identifier::MirrorChannel(mirror_channel),
                                    from: Identifier::Discord(GenericId(discord_identifier)),
                                    node_id,
                                } => {
                                    //TODO(superwhiskers): implement the handling of requests
                                    //                     to end publishing tasks. do not forget to ignore those from our own node
                                    warn!("ending publishing tasks is not supported yet, ignoring");
                                    rabbitmq_channel
                                            .basic_nack(potential_update.delivery_tag, BasicNackOptions::default())
                                            .unwrap_or_else(|err| {
                                                error!(
                                                    "unable to negative-acknowledge a NodeQueueUpdate::EndTask for a publishing task, error: {}",
                                                    err,
                                                )
                                            })
                                            .boxed()
                                },
                                NodeQueueUpdate::EndTask {
                                    to: Identifier::Discord(GenericId(discord_identifier)),
                                    from: Identifier::MirrorChannel(mirror_channel),
                                    node_id: origin_node_id,
                                } => {
                                    if origin_node_id == node_id {
                                        debug!("received a NodeQueueUpdate::EndTask for our own node");
                                        rabbitmq_channel
                                            .basic_nack(potential_update.delivery_tag, BasicNackOptions::default())
                                            .unwrap_or_else(|err| {
                                                error!(
                                                    "unable to negative-acknowledge a NodeQueueUpdate::EndTask for a publishing task, error: {}",
                                                    err,
                                                )
                                            })
                                            .boxed()
                                    } else {
                                        rabbitmq_channel
                                            .basic_ack(
                                                    potential_update.delivery_tag,
                                                    BasicAckOptions::default(),
                                            )
                                            .err_into::<anyhow::Error>()
                                            .and_then({
                                                let mirror_manager_channel = mirror_manager_channel.clone();
                                                let mirror_channel = mirror_channel.clone();
                                                move |_| {
                                                    future::ready(mirror_manager_channel
                                                        .send(
                                                            MirrorTaskSubscriptionUpdate::Remove(
                                                                ChannelId(discord_identifier),
                                                                mirror_channel.to_string()
                                                            )
                                                        ))
                                                        .err_into()
                                                }
                                            })
                                            .unwrap_or_else(move |err| {
                                                error!(
                                                    "unable to end a mirroring task, to: {}, from: {}, error: {}",
                                                    discord_identifier,
                                                    mirror_channel,
                                                    err,
                                                )
                                            })
                                            .boxed()
                                    }
                                },
                                NodeQueueUpdate::EndTask { to, from, node_id } => {
                                    error!(
                                        "unsupported to/from pair in a NodeQueueUpdate::EndTask, to: {}, from: {}",
                                        to,
                                        from
                                    );
                                    rabbitmq_channel
                                            .basic_nack(potential_update.delivery_tag, BasicNackOptions::default())
                                            .unwrap_or_else(|err| {
                                                error!(
                                                    "unable to negative-acknowledge a malformed NodeQueueUpdate::EndTask, error: {}",
                                                    err,
                                                )
                                            })
                                            .boxed()
                                },
                            },

                            Err(err) => {
                                error!("unable to parse data sent over the node queue, error: {}, data: {:?}", err, potential_update);
                                rabbitmq_channel
                                        .basic_nack(potential_update.delivery_tag, BasicNackOptions::default())
                                        .unwrap_or_else(|err| error!("unable to negative-acknowledge malformed data sent over the node queue, error: {}", err))
                                        .boxed()
                            },
                        }
                    }

                    Err(err) => {
                        error!("unable to consume messages from the node queue, error: {}", err);
                        future::ready(())
                            .boxed()
                    },
                }
            }),
    );

    //TODO(superwhiskers): move this outside of main() and spawn it with tokio::spawn(),
    //                     managing the asynchronous execution of it using the handle and
    //                     killing it on a signal such as SIGINT, SIGTERM, or SIGQUIT.
    //
    //                    consider also handling SIGHUP and respond by reconnecting to
    //                     discord
    //
    //                     https://docs.rs/signal-hook-tokio/0.3.0/signal_hook_tokio/index.html
    //                     will likely be the library used for handling this

    trace!("starting the main bot loop");

    while let Some((_shard_id, event)) = events.next().await {
        match event {
            Event::Ready(event) => info!(
                "logged in as {}#{}",
                event.user.name, event.user.discriminator
            ),
            Event::InteractionCreate(event) => {
                if let Interaction::ApplicationCommand(interaction) = event.0 {
                    match interaction.data.name.as_str() {
                        "about" => {
                            if let Err(err) =
                                commands::about_handler(Arc::clone(&http_client), interaction).await
                            {
                                error!(
                                    "unable to complete execution of the about command: {}",
                                    err
                                );
                            }
                        }
                        _ => (),
                    }
                }
            }
            _ => debug!("unhandled event: {:?}", event),
        }
    }

    Ok(())
}
