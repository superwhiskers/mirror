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

use backoff::{
    exponential::ExponentialBackoff, future::retry_notify, Error as BackoffError, SystemClock,
};
use deadpool_lapin::{Pool as LapinPool, PoolError as LapinPoolError};
use futures::{
    future::TryFutureExt,
    stream::{StreamExt, TryStreamExt},
};
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ExchangeDeclareOptions,
    },
    types::FieldTable,
    BasicProperties, Consumer, Error as LapinError, ExchangeKind,
};
use rmp_serde::{decode::Error as RmpDecodeError, encode::Error as RmpEncodeError};
use scylla::{
    cql_to_rust::FromRowError as ScyllaFromRowError,
    transport::{errors::QueryError as ScyllaQueryError, session::Session as ScyllaSession},
};
use std::{borrow::Cow, collections::HashMap, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{
    self,
    sync::{
        mpsc::{self, error::SendError},
        oneshot::{self, error::TryRecvError},
    },
    task::JoinHandle,
    time,
};
use tracing::{debug, error, trace, warn};
use twilight_gateway::cluster::Cluster;
use twilight_http::{error::Error as TwilightHttpError, Client as HttpClient};
use twilight_model::id::{marker as id_marker, Id};
use twilight_validate::message::MessageValidationError as TwilightMessageValidationError;
use uuid::Uuid;

use crate::{
    model::{
        identifier::Identifier,
        rabbitmq::{MirrorChannelStreamUpdate, NodeQueueUpdate},
        scylla::IncompleteMirrorChannelTableRow,
    },
    prepared_statements::{PreparedStatementKey, PreparedStatements},
};

/// A signal to drop the mirroring task associated with the (discord channel, mirror channel) pair
/// and handle the error, whatever it may be
pub struct ErroredMirrorTask(Id<id_marker::ChannelMarker>, Uuid, MirrorTaskError);

/// An enumeration over possible return values from mirroring tasks
#[derive(Error, Debug)]
pub enum MirrorTaskError {
    /// The database lacks a row for this specific service channel in the mirror channel's table
    #[error("The database lacks a row for the service channel a task was intended to mirror messages to")]
    NoMirrorChannelTableServiceChannelRow,

    /// The database returned nothing when trying to count the amount of rows returned from counting the bans a user has
    #[error("The database returned nothing when queried for the amount of bans a user has")]
    NothingReturnedWhenQueryingUserBans,

    /// The database returned a row with an [`IdentifierKind`] that does not make sense in the usercache
    #[error("The database returned a row with an originating service that does not make sense in the usercache")]
    NonsensicalIdentifierKindInUsercache,

    /// An error that may arise when using the twilight http client
    #[error("An error was encountered while working with the twilight http client")]
    TwilightHttpError(#[from] TwilightHttpError),

    /// An error that may arise when validating Discord request parameters
    #[error("An error was encountered while validating Discord request parameters")]
    TwilightMessageValidationError(#[from] TwilightMessageValidationError),

    /// The [`PreparedStatements`] map lacks a key
    #[error("The `PreparedStatements` map lacks the `{0:?}` key")]
    NoPreparedStatement(PreparedStatementKey),

    /// An error was encountered while querying the database
    #[error("An error was encountered while querying the database")]
    ScyllaQueryError(#[from] ScyllaQueryError),

    /// An error encountered while encoding to MessagePack
    #[error("An error was encountered while serializing to the MessagePack encoding")]
    RmpEncodeError(#[from] RmpEncodeError),

    /// An error encountered when decoding from MessagePack
    #[error("An error was encountered while deserializing from the MessagePack encoding")]
    RmpDecodeError(#[from] RmpDecodeError),

    /// An error encountered while trying to work with the RabbitMQ connection pool
    #[error("An error was encountered while trying to work with the RabbitMQ connection pool")]
    LapinPoolError(#[from] LapinPoolError),

    /// An error encountered while working with RabbitMQ
    #[error("An error was encountered while working with RabbitMQ")]
    LapinError(#[from] lapin::Error),

    /// An error encountered while trying to convert to a data structure from a scylla table row
    #[error("An error was encountered while trying to convert a table row into a data structure")]
    ScyllaFromRowError(#[from] ScyllaFromRowError),
}

/// An enumeration over possible updates to the task's internal channel list
#[derive(Debug)]
pub enum MirrorTaskSubscriptionUpdate {
    /// Add the specified discord channel to the subscription list for the
    /// specified mirror channel
    Add(Id<id_marker::ChannelMarker>, Uuid),

    /// Remove the specified discord channel from the subscription list for
    /// the specified mirror channel
    Remove(Id<id_marker::ChannelMarker>, Uuid),

    /// Stop all asynchronous mirroring tasks and exit
    Stop,
}

/// An enumeration over possible updates to the internal state of the stopped mirroring tasks
/// manager
#[derive(Debug)]
pub enum StoppedMirrorTasksUpdate {
    /// Add the specified task information to the list of tasks that were asked to stop
    Add {
        channel: (Id<id_marker::ChannelMarker>, Uuid),
        task: JoinHandle<()>,
    },

    /// Finish up waiting on stopped tasks and exit
    Stop,
}

pub async fn node_queue(
    node_id: Uuid,
    mut consumer: Consumer,
    mirror_manager_channel: mpsc::UnboundedSender<MirrorTaskSubscriptionUpdate>,
) {
    while let Some(potential_update) = consumer.next().await {
        match potential_update {
            Ok(potential_update) => {
                let update =
                    rmp_serde::from_read::<_, NodeQueueUpdate>(potential_update.data.as_slice());
                trace!("received node queue update event: {:?}", update);

                match update {
                    Ok(update) => match update {
                        NodeQueueUpdate::EndTask {
                            to: Identifier::MirrorChannel(mirror_channel),
                            from: Identifier::Discord(discord_identifier),
                            node_id,
                        } => {
                            //TODO(superwhiskers): implement the handling of requests
                            //                     to end publishing tasks. do not forget to ignore those from our own node
                            warn!("ending publishing tasks is not supported yet, ignoring");
                            if let Err(err) =
                                potential_update.nack(BasicNackOptions::default()).await
                            {
                                error!("unable to negative-acknowledge a NodeQueueUpdate::EndTask for a publishing task, error: {}", err);
                            }
                        }
                        NodeQueueUpdate::EndTask {
                            to: Identifier::Discord(discord_identifier),
                            from: Identifier::MirrorChannel(mirror_channel),
                            node_id: origin_node_id,
                        } => {
                            if origin_node_id == node_id {
                                debug!("received a NodeQueueUpdate::EndTask for a mirroring task from our own node");
                                if let Err(err) =
                                    potential_update.nack(BasicNackOptions::default()).await
                                {
                                    error!("unable to negative-acknowledge a NodeQueueUpdate::EndTask for a mirroring task, error: {}", err);
                                }
                            } else {
                                match potential_update.ack(BasicAckOptions::default()).await {
                                    Ok(_) => {
                                        if let Err(err) = mirror_manager_channel
                                            .send(MirrorTaskSubscriptionUpdate::Remove(
                                                discord_identifier.cast(),
                                                mirror_channel,
                                            )) {
                                            error!(
                                                "unable to send a request to end a mirroring task, to: {}, from: {}, origin: {}, error: {}",
                                                discord_identifier,
                                                mirror_channel,
                                                origin_node_id,
                                                err,
                                            )
                                        }
                                    }
                                    Err(err) => error!("unable to acknowledge a NodeQueueUpdate::EndTask for a mirroring task, error: {}", err),
                                }
                            }
                        }
                        NodeQueueUpdate::EndTask {
                            to,
                            from,
                            node_id: origin_node_id,
                        } => {
                            error!(
                                "unsupported to/from pair in a NodeQueueUpdate::EndTask, to: {}, from: {}, origin: {}",
                                to,
                                from,
                                origin_node_id,
                            );
                            if let Err(err) =
                                potential_update.nack(BasicNackOptions::default()).await
                            {
                                error!(
                                    "unable to negative-acknowledge a malformed NodeQueueUpdate::EndTask, error: {}",
                                    err,
                                )
                            }
                        }
                    },

                    Err(err) => {
                        error!(
                            "unable to parse data sent over the node queue, error: {}, data: {:?}",
                            err, potential_update
                        );
                        if let Err(err) = potential_update.nack(BasicNackOptions::default()).await {
                            error!("unable to negative-acknowledge malformed data sent over the node queue, error: {}", err)
                        }
                    }
                }
            }

            Err(err) => {
                error!(
                    "unable to consume messages from the node queue, error: {}",
                    err
                );
            }
        }
    }
}

/// An asynchronous task that mirrors messages to a discord channel from its corresponding mirror
/// channel
#[allow(clippy::too_many_arguments)]
pub async fn mirroring(
    errored: mpsc::UnboundedSender<ErroredMirrorTask>,
    mut stop_channel: oneshot::Receiver<()>,
    service_channel: Id<id_marker::ChannelMarker>,
    mirror_channel: Uuid,
    node_id: Uuid,
    http_client: Arc<HttpClient>,
    scylla: Arc<ScyllaSession>,
    rabbitmq: LapinPool,
    prepared_statements: Arc<PreparedStatements>,
) {
    if let Err(err) = try {
        debug!(
            "mirroring task spawned, service channel: {}, mirror channel: {}",
            service_channel, mirror_channel
        );

        //TODO(superwhiskers): look into fine-tuning the parameters for exponential backoff here.
        //                     there's a chance the defaults aren't "good enough"
        let rabbitmq_channel = retry_notify(
            ExponentialBackoff::<SystemClock>::default(),
            || {
                rabbitmq.get().map_err(|err| match err {
                    err @ LapinPoolError::Timeout(_) => BackoffError::Transient {
                        err,
                        retry_after: Some(Duration::from_secs(0)),
                    },

                    //TODO(superwhiskers): are there any other transient errors?
                    err => BackoffError::Permanent(err),
                })
            },
            |err, _| error!("unable to get a rabbitmq connection, error: {}", err),
        )
        .await?
        .create_channel()
        .await?;

        rabbitmq_channel
            .exchange_declare(
                "nodes",
                ExchangeKind::Direct,
                ExchangeDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        trace!("publishing to the nodes topic exchange");

        rabbitmq_channel
            .basic_publish(
                "nodes",
                "discord",
                BasicPublishOptions::default(),
                &rmp_serde::to_vec(&NodeQueueUpdate::EndTask {
                    to: Identifier::Discord(service_channel.cast()),
                    from: Identifier::MirrorChannel(mirror_channel),
                    node_id,
                })?,
                BasicProperties::default(),
            )
            .await?;

        trace!("querying the database");

        //TODO(superwhiskers): there appears to be a problem here related to the uuid field
        // -- this appears to be because we haven't updated the mirror channels to use
        //    "friendly names"
        let mirror_channel_info = scylla
            .execute(
                prepared_statements
                    .get(PreparedStatementKey::GetMirrorChannel)
                    .ok_or(MirrorTaskError::NoPreparedStatement(
                        PreparedStatementKey::GetMirrorChannel,
                    ))?,
                ("discord", mirror_channel, service_channel.to_string()),
            )
            .await?
            .rows
            .ok_or(MirrorTaskError::NoMirrorChannelTableServiceChannelRow)?
            .pop() //NOTE: there should only ever be one
            .ok_or(MirrorTaskError::NoMirrorChannelTableServiceChannelRow)?
            .into_typed::<IncompleteMirrorChannelTableRow>()?;

        trace!("creating the consumer");

        //TODO(superwhiskers): figure out if this is the best possible prefetch count or if we need to tweak it
        rabbitmq_channel
            .basic_qos(5, BasicQosOptions::default())
            .await?;

        let mut consume_options = FieldTable::default();
        consume_options.insert(
            "x-stream-offset".into(),
            mirror_channel_info.stream_offset.into(),
        );

        let mut consumer = rabbitmq_channel
            .basic_consume(
                ("messages.".to_owned() + mirror_channel.to_string().as_str()).as_str(),
                node_id.to_string().as_str(),
                BasicConsumeOptions::default(),
                consume_options,
            )
            .await?;

        trace!(
            "beginning main mirroring task loop from {} to {}",
            mirror_channel,
            service_channel
        );

        // really, if the channel is closed or we have a message, it is almost certainly
        // being instructed to stop
        while time::timeout(Duration::from_millis(10), &mut stop_channel)
            .await
            .is_err()
        {
            // i do not believe any error that can arise from this will ever be
            // transient, and i also don't want to deal with the borrow checker more
            // than i have to. if any error does turn out to be transient, then i will
            // promptly sort out the borrow checker stuff, but since i do not feel a
            // need to right now, it has not been done
            //
            // this also applies to the code consuming from a queue in main.rs
            while let Some(potential_update) = consumer.try_next().await? {
                let update = rmp_serde::from_read::<_, MirrorChannelStreamUpdate>(
                    potential_update.data.as_slice(),
                )?;
                trace!(
                    "received mirror channel stream update event, data: {:?}",
                    update
                );

                match update {
                    //MirrorChannelStreamUpdate::MirrorChannelConfiguration(_) => (),
                    //MirrorChannelStreamUpdate::ServiceChannelConfiguration(_) => (),
                    MirrorChannelStreamUpdate::Message { author, content } => {
                        trace!(
                            "got message update, author: {}, content: `{}`",
                            author,
                            content
                        );

                        if scylla
                            .execute(
                                prepared_statements
                                    .get(PreparedStatementKey::IsUserBanned)
                                    .ok_or(MirrorTaskError::NoPreparedStatement(
                                        PreparedStatementKey::IsUserBanned,
                                    ))?,
                                (&mirror_channel, author.to_string()),
                            )
                            .await?
                            .rows
                            .ok_or(MirrorTaskError::NothingReturnedWhenQueryingUserBans)?
                            .pop()
                            .ok_or(MirrorTaskError::NothingReturnedWhenQueryingUserBans)?
                            .into_typed::<(i64,)>()?
                            .0
                            == 0
                        {
                            debug!(
                                "sending message, author: {}, content: `{}`",
                                author, content
                            );

                            scylla
                                    .execute(
                                        prepared_statements
                                            .get(PreparedStatementKey::FetchUsernameFromUsercacheByUserAndService)
                                            .ok_or(MirrorTaskError::NoPreparedStatement(
                                                PreparedStatementKey::FetchUsernameFromUsercacheByUserAndService
                                            ))?,
                                            (match &author {
                                                Identifier::Discord(id) => id.to_string(),
                                                _ => Err(MirrorTaskError::NonsensicalIdentifierKindInUsercache)?,
                                            }, author.kind().as_str())
                                    ).await?
                                    .rows; //TODO(superwhiskers): find a way to handle this cleanly---probably through a branching path

                            http_client
                                .create_message(service_channel)
                                .content(&content)? //TODO(superwhiskers): add the username from usercache to this (consider adding settings for modifying the author display)
                                .exec()
                                .await?;

                            potential_update.ack(BasicAckOptions::default()).await?;
                        } else {
                            debug!(
                                "author is banned in this context, author: {}, context: {}",
                                author, mirror_channel
                            );

                            potential_update.ack(BasicAckOptions::default()).await?;
                        }

                        //TODO(superwhiskers): if we've made it this far, we should be able to safely update the offset somehow
                    }
                }
            }
        }
    } {
        debug!(
            "a mirroring task has errored, service channel: {}, mirrorchannel: {}, error: {:?}",
            service_channel, mirror_channel, err
        );

        if errored
            .send(ErroredMirrorTask(
                service_channel,
                mirror_channel.clone(),
                err,
            ))
            .is_err()
        {
            panic!("unable to send to the errored mirror task channel, service channel: {}, mirror channel: {}", service_channel, mirror_channel);
        }
    }

    debug!(
        "mirroring task stopped, service channel: {}, mirror channel: {}",
        service_channel, mirror_channel
    );
}

/// An asynchronous task for managing mirror tasks that have been signalled to stop
pub async fn stopped_mirror_manager(
    mut channel: mpsc::UnboundedReceiver<StoppedMirrorTasksUpdate>,
) {
    trace!("starting the stopped tasks manager");

    loop {
        //TODO(superwhiskers): currently we assume that the channel will never close without having
        //                     a `StoppedMirrorTasksUpdate::Stop` sent across it
        let event = time::timeout(Duration::from_millis(10), channel.recv()).await;

        if let Ok(Some(event)) = event {
            trace!("received stopped tasks event: {:?}", event);

            match event {
                StoppedMirrorTasksUpdate::Add {
                    channel: (service_channel, mirror_channel),
                    task: handle,
                } => {
                    debug!(
                        "got new stopped mirroring task, service channel: {}, mirror channel: {}",
                        service_channel, &mirror_channel
                    );

                    match time::timeout(Duration::from_millis(500), handle).await {
                        Ok(Ok(())) => debug!("a mirroring task was cleanly stopped, service channel: {}, mirror channel: {}", service_channel, mirror_channel),

                        Ok(Err(_)) => debug!("a mirroring task has either panicked or has been aborted, service channel: {}, mirror channel: {}", service_channel, mirror_channel),

                        Err(_) => debug!("a mirroring task took too long to stop and has been forcefully closed, service channel: {}, mirror channel: {}", service_channel, mirror_channel),
                    }
                }

                StoppedMirrorTasksUpdate::Stop => break,
            }
        }
    }
}

/// An asynchronous task for managing tasks that mirror messages across service channels
pub async fn mirror_manager(
    mut task_management: mpsc::UnboundedReceiver<MirrorTaskSubscriptionUpdate>,
    node_id: Uuid,
    rabbitmq: LapinPool,
    scylla: Arc<ScyllaSession>,
    http_client: Arc<HttpClient>,
    cluster: Arc<Cluster>,
    prepared_statements: Arc<PreparedStatements>,
) {
    fn handle_remove_task(
        service_channel: Id<id_marker::ChannelMarker>,
        mirror_channel: Uuid,
        handle: JoinHandle<()>,
        stop_sender: oneshot::Sender<()>,
        stopped_tasks: &mpsc::UnboundedSender<StoppedMirrorTasksUpdate>,
    ) {
        if stop_sender.send(()).is_err() {
            // this should not happen, but we will handle it by not pushing the task's
            // data to the "stopping" group, as we assume that if the task has closed
            // its channel, it isn't going to stop on its own, so we instead abort it
            error!("the task mirroring messages to the {:?} service channel from the {} mirror channel has closed its stop channel receiver", service_channel, mirror_channel);

            handle.abort();
        } else if let Err(SendError(StoppedMirrorTasksUpdate::Add { task: handle, .. })) =
            stopped_tasks.send(StoppedMirrorTasksUpdate::Add {
                channel: (service_channel, mirror_channel),
                task: handle,
            })
        {
            error!("the task managing stopped mirroring tasks has closed its channel. forcefully closing the mirroring task");
            handle.abort();
            warn!("currently, there is no further handling of this error case. please report this event so that there is an incentive to further handle this");

            //TODO(superwhiskers): ideally, we would make an effort to restart the manager
            //                     task, but for now, this *should* handle the case decently
            //                     enough
        }
    }

    trace!("starting the mirror task manager");

    let mut tasks = HashMap::new();

    trace!("spawning the stopped tasks manager");

    let (errored_mirror_tasks, mut errored_mirror_tasks_receiver) = mpsc::unbounded_channel();
    let (stopped_tasks, stopped_tasks_receiver) = mpsc::unbounded_channel();
    let manager_handle = tokio::spawn(stopped_mirror_manager(stopped_tasks_receiver));

    loop {
        tokio::select! {
            Some(event) = task_management.recv() => {
                trace!("received task management event: {:?}", event);

                match event {
                    /*

                    spawn a new task managing the channel and open two channels

                    for the first channel, the sending end is held. this channel is used to signal
                    the corresponding task to stop (politely.)

                    for the second, we hold the receiving end. this channel is used by the task to
                    signal that it has stopped. if after a period of time, the task has still not
                    signalled to us that it has stopped, we forcefully abort it

                    we also hold onto the task's handle, to forcefully abort it if the case described
                    above occurs

                    */
                    MirrorTaskSubscriptionUpdate::Add(service_channel, mirror_channel) => {
                        // fast path: if the channel is already having messages from the exact same
                        // mirror channel mirrored to it, we skip starting another mirroring task
                        // to it
                        tasks.entry((service_channel, mirror_channel.clone())).or_insert_with(|| {
                            let (stop_sender, stop_receiver) = oneshot::channel();
                            (
                                tokio::spawn(mirroring(
                                    errored_mirror_tasks.clone(),
                                    stop_receiver,
                                    service_channel,
                                    mirror_channel,
                                    node_id,
                                    Arc::clone(&http_client),
                                    Arc::clone(&scylla),
                                    rabbitmq.clone(),
                                    Arc::clone(&prepared_statements),
                                )),
                                stop_sender,
                            )
                        });
                    }

                    /*

                    signal the existing task to stop and add its handle and our receiver to `stopped`

                    every iteration we check to see if any tasks in there have stopped themselves. if
                    they haven't after a certain amount of time has passed, we will forcefully abort
                    them

                    if after the manager itself has been signalled to stop, we still have some left
                    after waiting for a period of time, we will abort all of them

                    */
                    MirrorTaskSubscriptionUpdate::Remove(service_channel, mirror_channel) => {
                        let (handle, stop_sender) =
                            match tasks.remove(&(service_channel, mirror_channel.clone())) {
                                Some(removed_subscription) => removed_subscription,
                                None => return,
                            };
                        handle_remove_task(
                            service_channel,
                            mirror_channel,
                            handle,
                            stop_sender,
                            &stopped_tasks,
                        )
                    }

                    /*

                    signal all tasks to stop and wait for a period of time. if after this period has
                    passed, some tasks still have not exited, we abort all of them

                    */
                    MirrorTaskSubscriptionUpdate::Stop => break,
                }
            },
            Some(error) = errored_mirror_tasks_receiver.recv() => (), /* TODO(superwhiskers): sort this out */

            //TODO(superwhiskers): we may want better error reporting here
            else => break,
        }
    }

    trace!("stopping the mirror task manager");

    for ((service_channel, mirror_channel), (handle, stop_sender)) in tasks {
        handle_remove_task(
            service_channel,
            mirror_channel,
            handle,
            stop_sender,
            &stopped_tasks,
        );
    }

    if stopped_tasks.send(StoppedMirrorTasksUpdate::Stop).is_err() {
        warn!("the task managing stopped mirroring tasks has closed its channel. it will be terminated through its handle. this *may* leave some additional tasks alive");

        manager_handle.abort();
        return;
    }

    if let Err(join_error) = manager_handle.await {
        match join_error {
            _ if join_error.is_cancelled() => warn!("the stopped mirroring tasks manager was cancelled. this should not happen"),
            _ if join_error.is_panic() => warn!("the stopped mirroring tasks manager panicked. this should not happen"),

            // i could have used an unreachable! here, but this could change with updates to
            // tokio. so much for avoiding "breaking changes" by not using enums as error
            // types. this creates the same effect as using #[non_exhaustive]. why that isn't
            // used is beyond me
            _ => error!("the join error returned by the stopped mirroring tasks manager was of an unknown variant"),
        }
    }
}
