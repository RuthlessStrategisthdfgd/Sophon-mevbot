use std::net::SocketAddr;

use primitives::Address;
use secp256k1::{generate_keypair, PublicKey, Secp256k1, SecretKey};
use serial_test::serial;
use storage::storage_utils::remove_versa_data_dir;
use tokio::sync::mpsc::channel;
use versa_core::transactions::Token;
use versa_rpc::rpc::{JsonRpcServer, JsonRpcServerConfig};
use wallet::v2::{Wallet, WalletConfig};

#[tokio::test]
#[serial]
pub async fn create_wallet_with_rpc_client() {
    remove_versa_data_dir();
    let _: SocketAddr = "127.0.0.1:9293"
        .parse()
        .expect("Unable to create Socket Address");

    // Set up RPC Server to accept connection from client
    let json_rpc_server_config = JsonRpcServerConfig::default();

    let (server_handle, _) = JsonRpcServer::run(&json_rpc_server_config).await.unwrap();

    tokio::spawn(server_handle.stopped());

    let wallet_config = WalletConfig::default();

    Wallet::new(wallet_config).await.unwrap();
}

#[tokio::test]
#[serial]
pub async fn wallet_sends_txn_to_rpc_server() {
    remove_versa_data_dir();
    let _: SocketAddr = "127.0.0.1:9293"
        .parse()
        .expect("Unable to create Socket Address");

    let (events_tx, _events_rx) = channel(100);

    // Set up RPC Server to accept connection from client
    let json_rpc_server_config = JsonRpcServerConfig {
        events_tx,
        ..Default::default()
    };

    let (handle, socket_addr) = JsonRpcServer::run(&json_rpc_server_config).await.unwrap();

    tokio::spawn(handle.stopped());

    let wallet_config = WalletConfig {
        rpc_server_address: socket_addr,
        ..Default::default()
    };

    let mut wallet = Wallet::new(wallet_config).await.unwrap();

    type H = secp256k1::hashes::sha256::Hash;

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_hashed_data::<H>(b"versa");
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);

    wallet.create_account(0, public_key).await.unwrap();

    let timestamp = 0;

    let recv_sk = SecretKey::from_hashed_data::<H>(b"recv_versa");
    let recv_pk = PublicKey::from_secret_key(&secp, &recv_sk);

    let txn_digest = wallet
        .send_transaction(0, Address::new(recv_pk), 10, Token::default(), timestamp)
        .await
        .unwrap();

    assert_eq!(
        &txn_digest.to_string(),
        "bcbe339ef7e59edcc2811104896beb80e6212503143b9f2b799355a312e9f8d0"
    );
}

#[tokio::test]
#[serial]
pub async fn wallet_sends_create_account_request_to_rpc_server() {
    remove_versa_data_dir();
    let _: SocketAddr = "127.0.0.1:9293"
        .parse()
        .expect("Unable to create Socket Address");

    let (events_tx, _events_rx) = channel(100);

    // Set up RPC Server to accept connection from client
    let json_rpc_server_config = JsonRpcServerConfig {
        events_tx,
        ..Default::default()
    };

    let (handle, socket_addr) = JsonRpcServer::run(&json_rpc_server_config).await.unwrap();

    tokio::spawn(handle.stopped());

    let wallet_config = WalletConfig {
        rpc_server_address: socket_addr,
        ..Default::default()
    };

    let mut wallet = Wallet::new(wallet_config).await.unwrap();

    let (_, public_key) = generate_keypair(&mut rand::thread_rng());

    wallet.create_account(1, public_key).await.unwrap();
}
