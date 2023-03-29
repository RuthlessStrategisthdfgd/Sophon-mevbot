use std::{collections::HashSet, net::SocketAddr, result::Result as StdResult};

use async_trait::async_trait;
use bytes::Bytes;
use events::{DirectedEvent, Event};
use network::{
    message::{Message, MessageBody},
    network::{BroadcastEngine, ConnectionIncoming},
};
use primitives::{NodeType, PeerId};
use telemetry::{error, info, warn};
use theater::{ActorLabel, ActorState, Handler};
use tokio::{
    sync::{
        broadcast::{
            error::{RecvError, TryRecvError},
            Receiver,
        },
        mpsc::Sender,
    },
    task::JoinHandle,
};
use uuid::Uuid;

use crate::{EventBroadcastReceiver, EventBroadcastSender, NodeError, Result, RuntimeModule};

pub const BROADCAST_CONTROLLER_BUFFER_SIZE: usize = 10000;

/// The number of erasures that the raptorq encoder will use to encode the
/// block.
const RAPTOR_ERASURE_COUNT: u32 = 3000;

#[derive(Debug)]
pub struct BroadcastEngineController {
    addr: SocketAddr,
    engine: BroadcastEngine,
    events_tx: EventBroadcastSender,
    // events_rx: EventBroadcastReceiver,
}

#[derive(Debug)]
pub struct BroadcastEngineControllerConfig {
    pub engine: BroadcastEngine,
    pub events_tx: EventBroadcastSender,
}

impl BroadcastEngineController {
    pub fn new(config: BroadcastEngineControllerConfig) -> Self {
        let engine = config.engine;
        let addr = engine.local_addr();
        let events_tx = config.events_tx;

        Self {
            engine,
            addr,
            events_tx,
        }
    }

    pub async fn listen(
        &mut self,
        mut events_rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
    ) -> Result<()> {
        loop {
            tokio::select! {
                Some((conn, conn_incoming)) = self.engine.get_incoming_connections().next() => {
                match self.map_network_conn_to_message(conn_incoming).await {
                    Ok(message) => {
                        self.handle_network_event(message).await;
                    },
                     Err(err) => {
                        error!("unable to map connection into message: {err}");
                    }
                  }

                },
                Some(event) = events_rx.recv() => {
                    if matches!(event, Event::Stop) {
                        info!("Stopping broadcast controller");
                        break
                        // return Ok(());
                    }
                    self.handle_internal_event(event).await;
                },
                // else => break,
            };
        }

        Ok(())
    }

    async fn handle_network_event(&self, message: Message) -> Result<()> {
        match message.data {
            MessageBody::InvalidBlock { .. } => {},
            MessageBody::Disconnect { .. } => {},
            MessageBody::StateComponents { .. } => {},
            MessageBody::Genesis { .. } => {},
            MessageBody::Child { .. } => {},
            MessageBody::Parent { .. } => {},
            MessageBody::Ledger { .. } => {},
            MessageBody::NetworkState { .. } => {},
            MessageBody::ClaimAbandoned { .. } => {},
            MessageBody::ResetPeerConnection { .. } => {},
            MessageBody::RemovePeer { .. } => {},
            MessageBody::AddPeer { .. } => {},
            MessageBody::DKGPartCommitment {
                part_commitment,
                sender_id,
            } => {},
            MessageBody::DKGPartAcknowledgement { .. } => {},
            MessageBody::Vote { .. } => {},
            MessageBody::Empty => {},
        };
        //
        //
        // if let Err(err) = tx.send(body.into()).await {
        //     error!("failed to forward data received from network:
        // {err}"); }
        //
        //
        //
        Ok(())
    }

    async fn handle_internal_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Stop => Ok(()),
            Event::PartMessage(sender_id, part_commitment) => {
                let status = self
                    .engine
                    .quic_broadcast(Message::new(MessageBody::DKGPartCommitment {
                        sender_id,
                        part_commitment,
                    }))
                    .await?;

                info!("Broadcasted part commitment to peers: {status:?}");
                Ok(())
            },
            Event::SyncPeers(peers) => {
                if peers.is_empty() {
                    warn!("No peers to sync with");

                    // TODO: revisit this return
                    return Ok(());
                }

                let mut quic_addresses = vec![];
                let mut raptor_peer_list = vec![];

                for peer in peers.iter() {
                    let addr = peer.address;

                    quic_addresses.push(addr);

                    let mut raptor_addr = addr;
                    raptor_addr.set_port(peer.raptor_udp_port);
                    raptor_peer_list.push(raptor_addr);
                }

                self.engine.add_raptor_peers(raptor_peer_list);

                let status = self.engine.add_peer_connection(quic_addresses).await?;

                info!("{status:?}");

                Ok(())
            },
            Event::Vote(vote, quorum_type, farmer_quorum_threshold) => {
                let status = self
                    .engine
                    .quic_broadcast(Message::new(MessageBody::Vote {
                        vote,
                        quorum_type,
                        farmer_quorum_threshold,
                    }))
                    .await?;

                info!("{status:?}");

                Ok(())
            },
            // Broadcasting the Convergence block to the peers.
            Event::BlockConfirmed(block) => {
                let status = self
                    .engine
                    .unreliable_broadcast(block, RAPTOR_ERASURE_COUNT, self.engine.raptor_udp_port)
                    .await?;

                info!("{status:?}");

                Ok(())
            },
            _ => Ok(()),
        }
    }

    /// Turns connection data into Message then returns it
    async fn map_network_conn_to_message(
        &self,
        mut conn_incoming: ConnectionIncoming,
    ) -> Result<Message> {
        let res = conn_incoming.next().await.map_err(|err| {
            NodeError::Other(format!("unable to listen for new connections: {err}"))
        })?;

        let (_, _, raw_message) = res.unwrap_or((Bytes::new(), Bytes::new(), Bytes::new()));
        let message = Message::from(raw_message.to_vec());

        return Ok(message);
    }
}
