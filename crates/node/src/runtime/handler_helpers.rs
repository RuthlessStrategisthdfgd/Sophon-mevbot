use block::{
    header::BlockHeader, Block, Certificate, ConvergenceBlock, GenesisBlock, ProposalBlock, RefHash,
};
use events::{AccountBytes, AssignedQuorumMembership, Event, PeerData, Vote};
use miner::conflict_resolver::Resolver;
use primitives::{Address, Epoch, NodeId, NodeType, QuorumKind, RawSignature, Round, Signature};
use quorum::quorum::Quorum;
use signer::engine::{SignerEngine, VALIDATION_THRESHOLD};
use std::collections::HashMap;
use storage::vrrbdb::ApplyBlockResult;
use vrrb_core::claim::Claim;

use crate::{
    node_runtime::NodeRuntime,
    result::{NodeError, Result},
};

pub const PULL_TXN_BATCH_SIZE: usize = 100;

impl NodeRuntime {
    pub fn handle_block_received(&mut self, block: Block) -> Result<ApplyBlockResult> {
        match block {
            Block::Genesis { block } => self.handle_genesis_block_received(block),
            Block::Proposal { block } => self.handle_proposal_block_received(block),
            Block::Convergence { block } => self.handle_convergence_block_received(block),
        }
    }

    fn handle_genesis_block_received(&mut self, block: GenesisBlock) -> Result<ApplyBlockResult> {
        // TODO: append blocks to only one instance of the DAG
        self.state_driver
            .dag
            .append_genesis(&block)
            .map_err(|err| {
                NodeError::Other(format!("Failed to append genesis block to DAG: {err:?}"))
            })?;

        let apply_result = self.state_driver.apply_block(Block::Genesis { block })?;

        Ok(apply_result)
    }

    fn handle_proposal_block_received(&mut self, block: ProposalBlock) -> Result<ApplyBlockResult> {
        if let Err(e) = self
            .state_driver
            .dag
            .append_proposal(&block, self.consensus_driver.sig_engine.clone())
        {
            let err_note = format!("Failed to append proposal block to DAG: {e:?}");
            return Err(NodeError::Other(err_note));
        }
        todo!()
    }

    /// Certifies and stores a convergence block within a node's state if certification succeeds
    fn handle_convergence_block_received(
        &mut self,
        mut block: ConvergenceBlock,
    ) -> Result<ApplyBlockResult> {
        self.has_required_node_type(NodeType::Validator, "certify convergence block")?;
        self.belongs_to_correct_quorum(QuorumKind::Harvester, "certify convergence block")?;

        self.state_driver
            .dag
            .append_convergence(&mut block)
            .map_err(|err| {
                NodeError::Other(format!(
                    "Could not append convergence block to DAG: {err:?}"
                ))
            })?;

        if block.certificate.is_none() {
            if let Some(_) = self.state_driver.dag.last_confirmed_block_header() {
                let certificate = self.certify_convergence_block(block.clone());
            }
        }

        let apply_result = self
            .state_driver
            .apply_block(Block::Convergence { block })?;

        Ok(apply_result)
    }

    pub async fn handle_harvester_signature_received(
        &mut self,
        block_hash: String,
        node_id: NodeId,
        sig: Signature,
    ) -> Result<Certificate> {
        self.consensus_driver
            .sig_engine
            .verify(&node_id, &sig, &block_hash)
            .map_err(|err| NodeError::Other(err.to_string()))?;
        let set = self
            .state_driver
            .dag
            .add_signer_to_convergence_block(
                block_hash.clone(),
                sig,
                node_id,
                &self.consensus_driver.sig_engine,
            )
            .map_err(|err| NodeError::Other(err.to_string()))?;
        if set.len()
            < self
                .consensus_driver
                .sig_engine
                .quorum_members()
                .get_harvester_threshold()
        {
            return Err(NodeError::Other(format!("threshold not reached yet")));
        }

        let sig_set = set.into_iter().collect();
        let cert = self
            .form_convergence_certificate(block_hash, sig_set)
            .map_err(|err| NodeError::Other(err.to_string()))?;

        self.events_tx
            .send(Event::BlockCertificateCreated(cert.clone()).into())
            .await
            .map_err(|err| NodeError::Other(err.to_string()))?;
        Ok(cert)
    }

    pub fn form_convergence_certificate(
        &mut self,
        block_hash: String,
        sigs: Vec<(NodeId, Signature)>,
    ) -> Result<Certificate> {
        // TODO: figure out how to get next_root_hash back into cert
        // this should probably be part of the signature process
        self.consensus_driver.is_harvester()?;
        self.consensus_driver
            .sig_engine
            .verify_batch(&sigs, &block_hash)
            .map_err(|err| NodeError::Other(err.to_string()))?;
        if let Some(ref mut block) = self
            .state_driver
            .dag
            .get_pending_convergence_block_mut(&block_hash)
        {
            let root_hash = block.header.txn_hash.clone();
            let block_hash = block.hash.clone();
            let inauguration = if let Some(quorum) = &self.quorum_pending {
                Some(quorum.clone())
            } else {
                None
            };
            let cert = Certificate {
                signatures: sigs,
                //TODO: handle inauguration blocks
                inauguration: inauguration.clone(),
                root_hash,
                block_hash: block_hash.clone(),
            };
            if let Some(quorum_members) = inauguration {
                self.consensus_driver.sig_engine.set_quorum_members(
                    quorum_members
                        .0
                        .into_iter()
                        .map(|(_, data)| {
                            (data.quorum_kind, data.members.clone().into_iter().collect())
                        })
                        .collect(),
                );
                self.quorum_pending = None;
            }
            Ok(cert)
        } else {
            Err(NodeError::Other(format!(
                "unable to find convergence block: {} in pending convergence blocks in dag",
                block_hash.clone()
            )))
        }
    }

    // harvester sign and create cert
    pub fn handle_block_certificate_created(&mut self, certificate: Certificate) -> Result<()> {
        //TODO: implement logic under new model
        //apply block to state
        Ok(())
    }

    pub async fn handle_quorum_formed(&mut self) -> Result<()> {
        todo!();
    }

    // recieve cert from network
    pub async fn handle_block_certificate(&mut self, certificate: Certificate) -> Result<()> {
        //TODO: implement logic under new model
        // redundant can probably delete
        Ok(())
    }

    pub async fn handle_vote_received(&mut self, vote: Vote) -> Result<()> {
        self.consensus_driver.handle_vote_received(vote).await
    }

    pub async fn handle_node_added_to_peer_list(
        &mut self,
        peer_data: PeerData,
    ) -> Result<Option<HashMap<NodeId, AssignedQuorumMembership>>> {
        self.consensus_driver
            .handle_node_added_to_peer_list(peer_data)
            .await
    }

    pub fn handle_quorum_membership_assigment_created(
        &mut self,
        assigned_membership: AssignedQuorumMembership,
    ) -> Result<()> {
        self.consensus_driver
            .handle_quorum_membership_assigment_created(assigned_membership)
    }

    pub fn handle_quorum_membership_assigments_created(
        &mut self,
        assigned_membership: Vec<AssignedQuorumMembership>,
    ) -> Result<()> {
        self.consensus_driver
            .handle_quorum_membership_assigments_created(
                assigned_membership,
                self.config.id.clone(),
            )
    }

    pub async fn handle_convergence_block_precheck_requested<
        R: Resolver<Proposal = ProposalBlock>,
    >(
        &mut self,
        block: ConvergenceBlock,
        last_confirmed_block_header: BlockHeader,
        resolver: R,
    ) -> Result<()> {
        self.consensus_driver.is_harvester()?;
        match self.consensus_driver.precheck_convergence_block(
            block.clone(),
            last_confirmed_block_header,
            resolver,
            self.state_driver.dag.dag(),
        ) {
            Ok((true, true)) => {
                self.events_tx
                    .send(Event::SignConvergenceBlock(block.clone()).into())
                    .await
                    .map_err(|err| NodeError::Other(err.to_string()))?;
                Ok(())
            },
            Err(err) => return Err(NodeError::Other(err.to_string())),
            _ => {
                return Err(NodeError::Other(
                    "convergence block is not valid".to_string(),
                ))
            },
        }
    }

    pub async fn handle_sign_convergence_block(
        &mut self,
        block: ConvergenceBlock,
    ) -> Result<Signature> {
        self.consensus_driver.is_harvester()?;
        self.consensus_driver
            .sig_engine
            .sign(&block.hash)
            .map_err(|err| {
                NodeError::Other(format!(
                    "could not generate partial_signature on block: {}. err: {}",
                    block.hash.clone(),
                    err
                ))
            })
    }

    pub fn handle_quorum_election_started(&mut self, header: BlockHeader) -> Result<Vec<Quorum>> {
        let claims = self.state_driver.read_handle().claim_store_values();
        let quorum = self
            .consensus_driver
            .handle_quorum_election_started(header, claims)?;

        Ok(quorum)
    }

    pub fn handle_create_account_requested(
        &mut self,
        address: Address,
        account_bytes: AccountBytes,
    ) -> Result<()> {
        let account = bincode::deserialize(&account_bytes).map_err(|err| {
            NodeError::Other(format!("unable to deserialize account bytes: {err}"))
        })?;

        self.state_driver.insert_account(address, account)
    }
}
