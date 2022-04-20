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

use anyhow::{anyhow, Context, Result};
use deadpool_lapin::{
    lapin::{Connection, ConnectionProperties},
    Config as LapinPoolConfig, Manager as LapinPoolManager, Pool as LapinPool,
    PoolConfig as DeadpoolConfig, Runtime as DeadpoolRuntime,
};
use fixed_map::Map;
use futures::future::{self, Future, FutureExt, TryFutureExt};
use scylla::{
    statement::prepared_statement::PreparedStatement,
    transport::{
        session::{Session as ScyllaSession, SessionConfig as ScyllaSessionConfig},
        Compression as ScyllaSessionCompression,
    },
};
use std::sync::Arc;
use tokio::{net, sync::mpsc, task::JoinHandle};
use tracing::{debug, trace, warn};
use twilight_gateway::{
    cluster::{scheme::ShardScheme, Cluster, Events},
    Intents,
};
use twilight_http::client::{Client, InteractionClient};
use twilight_model::{
    application::command::Command,
    gateway::{
        payload::outgoing::update_presence::UpdatePresencePayload,
        presence::{ActivityType, MinimalActivity, Status},
    },
    oauth::Application,
};
use uuid::Uuid;

use crate::{
    commands::COMMANDS,
    errors::Error as MirrorError,
    model::configuration::Configuration,
    prepared_statements::{self, PreparedStatements},
    tasks::{self, MirrorTaskSubscriptionUpdate},
};

/// Initializes the `deadpool-lapin` library based on the provided configuration
pub async fn rabbitmq(config: &Configuration) -> Result<LapinPool> {
    trace!("creating the rabbitmq connection pool");

    let mut working_addresses = future::join_all(config.rabbitmq.addresses.iter().map(|address| {
        future::join(
            future::ready(address),
            Connection::connect(address, ConnectionProperties::default()),
        )
    }))
    .await
    .into_iter()
    .filter_map(|(address, potential_connection)| {
        potential_connection.ok().map(|_| address.as_str())
    });

    let node = working_addresses
        .next()
        .ok_or_else(|| anyhow!("no addresses were listed in the configuration"))?
        .to_string();

    debug!("chosen rabbitmq node: {:?}", node);

    //TODO(superwhiskers): find a way to pass along a client certificate as an identity
    Ok(
        LapinPool::builder(LapinPoolManager::new(node, ConnectionProperties::default()))
            .max_size(config.rabbitmq.max)
            .runtime(DeadpoolRuntime::Tokio1)
            .build()?,
    )
}

/// Initializes the `scylla` library based on the provided configuration
pub async fn scylla(
    config: &Configuration,
) -> Result<(Arc<ScyllaSession>, Arc<PreparedStatements>)> {
    trace!("initializing the scylladb client");

    let mut session_config = ScyllaSessionConfig::new();

    session_config.add_known_nodes(&config.scylla.addresses);
    session_config.compression = Some(ScyllaSessionCompression::Lz4);
    session_config.used_keyspace = Some("mirrorbot".to_string());
    session_config.auth_username = config.scylla.username.clone();
    session_config.auth_password = config.scylla.password.clone();

    let session = ScyllaSession::connect(session_config).await?;

    let mut prepared_statements = Map::new();
    for (key, statement) in prepared_statements::PREPARED_STATEMENTS.iter() {
        debug!(
            "initializing prepared statement, key: {:?}, statement: `{}`",
            key, statement
        );
        prepared_statements.insert(key, session.prepare(*statement).await?);
    }

    Ok((Arc::new(session), Arc::new(prepared_statements)))
}

/// Initializes the discord http client with the application id
pub async fn discord_http(config: &Configuration) -> Result<(Arc<Client>, Application)> {
    trace!("creating the discord http client");

    let client = Client::new(config.general.token.clone());

    trace!("retrieving application information");

    let application_info = client
        .current_user_application()
        .exec()
        .await
        .context("failed to retrieve the current application's information")?
        .model()
        .await
        .context("failed to retrieve a model from the returned application information")?;

    debug!("retrieved application information: {:?}", application_info);

    trace!("finished creating the discord http client");

    Ok((Arc::new(client), application_info))
}

/// Registers the bot's slash commands with discord
pub async fn discord_commands(
    client: &InteractionClient<'_>,
    application_info: &Application,
) -> Result<()> {
    trace!("registering the bot's commands with discord");

    warn!("TODO: remove the hardcoded guild id! this should only be used during development");

    //TODO: remove the hardcoded guild id and make these global
    let guild_id = 855614556990210068_u64
        .try_into()
        .expect("this should be impossible");

    client
        .set_guild_commands(
            guild_id,
            &COMMANDS
                .iter()
                .map(|c| Command {
                    application_id: Some(application_info.id),
                    guild_id: Some(guild_id),
                    name: c.name.to_string(),
                    default_permission: Some(c.default_permission),
                    description: c.description.to_string(),
                    id: None,
                    kind: c.kind,
                    options: c.options.to_vec(),
                    version: 1_u64.try_into().expect("this should be impossible"),
                })
                .collect::<Vec<Command>>(),
        )
        .exec()
        .await
        .context("failed to set the global discord bot commands")?;

    trace!("registered bot commands with discord");

    Ok(())
}

/// Initializes the discord gateway client
pub async fn discord_gateway(config: &Configuration) -> Result<(Arc<Cluster>, Events)> {
    trace!("initializing the gateway clients");

    let (cluster, events) = Cluster::builder(config.general.token.clone(), Intents::GUILD_MESSAGES)
        .presence(UpdatePresencePayload::new(
            vec![MinimalActivity {
                kind: ActivityType::Custom,
                name: "running experiments. not accepting commands".to_string(),
                url: None,
            }
            .into()],
            false,
            None,
            Status::DoNotDisturb,
        )?)
        .build()
        .await
        .context("failed to set up a discord gateway connection")?;

    let cluster = Arc::new(cluster);

    debug!("initialized the gateway clients: {:?}", cluster);
    trace!("connecting to the gateway");

    cluster.up().await;

    Ok((cluster, events))
}

/// Complete bot database initialization actions
pub async fn mirroring_tasks(
    node_id: Uuid,
    rabbitmq: LapinPool,
    scylla: Arc<ScyllaSession>,
    http_client: Arc<Client>,
    cluster: Arc<Cluster>,
    prepared_statements: Arc<PreparedStatements>,
) -> Result<(
    JoinHandle<()>,
    mpsc::UnboundedSender<MirrorTaskSubscriptionUpdate>,
)> {
    let mut rabbitmq_conn = rabbitmq
        .get()
        .await
        .context("failed to get a rabbitmq connection from the pool")?;

    // somewhere here we should create an unbounded asynchronous channel and spawn a task with
    // tokio::spawn(), providing it the receiving end of the channel. the sender end of the
    // channel should be then be used to feed the task channels and their associated groups to
    // follow.

    trace!("initializing mirroring task");

    let (mirror_manager_handle, mirror_manager_channel) = {
        let (channel_sender, channel_receiver) = mpsc::unbounded_channel();

        (
            tokio::spawn(tasks::mirror_manager(
                channel_receiver,
                node_id,
                rabbitmq.clone(),
                scylla,
                Arc::clone(&http_client),
                Arc::clone(&cluster),
                prepared_statements,
            )),
            channel_sender,
        )
    };

    trace!("retrieving mirror channels");

    //TODO(superwhiskers): strip this code as it's only used to verify that this works
    mirror_manager_channel.send(MirrorTaskSubscriptionUpdate::Add(
        855615515199537162_u64.try_into()?,
        "3c08f3c512774332a40610aa4d6186c2".parse()?, //TODO(superwhiskers): change these
    ))?;

    /*
    let channels = redis_conn
        .keys::<&str, Vec<String>>("messages-*")
        .await
        .context("unable to pull available message streams from the database")?;

    debug!("retrieved mirror channels: {:?}", channels);

    let conn_arena = Arena::with_capacity(channels.len());

    let streams = for (channel, groups) in
        future::try_join_all(channels.into_iter().map(|channel| {
            future::try_join(
                future::ok(channel.clone()),
                redis
                    .get()
                    .err_into::<anyhow::Error>()
                    .and_then(|redis_conn| {
                        conn_arena
                            .alloc(redis_conn)
                            .xinfo_groups::<String, Vec<Vec<String>>>(channel)
                            .err_into()
                    }),
            )
        }))
        .await?
    {
        // here we are going to iterate over the groups and determine which of them are
        // ones we are intended to watch over (using the algorithm described here:
        // https://discord.com/developers/docs/topics/gateway#sharding-sharding-formula)

        /*match cluster.config().shard_scheme() {
            // automatic sharding goes with all of the shards suggested by discord, rendering
            // filtering unnecessary as all of the shards will be available through the
            // cluster and we should be able to manage all of the channels
            ShardScheme::Auto => groups,

            //TODO(superwhiskers): handle other sharding schemes. they are not hand
            // ShardScheme::Bucket => {},
            // ShardScheme::Range => {},
            s => return Err(MirrorError::UnknownShardingScheme(s)),
        }*/

        println!("channel: {}", channel);
        println!("groups: {:?}", groups);

        ()
    };

    println!("{:?}", streams);
    */

    Ok((mirror_manager_handle, mirror_manager_channel))
}
