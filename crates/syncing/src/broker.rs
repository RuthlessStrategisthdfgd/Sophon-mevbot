
use std::{
    net::{
        IpAddr
    },
    fs::{File, self},
    io::{SeekFrom, Read, Seek},
    path::Path,
};

use log::{info, error, debug};
use udt::UdtError;

use crate::{
    transfer::{DATAClient, DATAServer, DATAClientConnection},
    context::ContextHandler,
};

pub struct LocalstateBroker {
    pub broker: DATAServer,
}

impl LocalstateBroker {
    pub fn new(
        context: ContextHandler<'static>,
    ) -> Self {

        LocalstateBroker {

            broker: DATAServer::new(
                context.get().borrow().broker_edge.local_ip().clone(),
                context.get().borrow().broker_edge.port(),
            ),
        }
    }    
}

pub struct LocalstateBrokerClient {
    pub node_syncer: DATAClient
}

impl LocalstateBrokerClient {
    pub fn new(ip_addr: IpAddr, port: u16) -> Self {

        let data_client = DATAClient::new_from_ip_addr(ip_addr, port);

        LocalstateBrokerClient {

            node_syncer: data_client
        }
    }
    
    pub fn connect_client(&self) -> Result<DATAClientConnection, UdtError> {
        self.node_syncer.connect()
    }
}

/// Data retrieval, all the operation in the current thread
pub fn broker_retrieve(
    context: ContextHandler<'_>,
    node_addr: IpAddr
) -> Result<usize, UdtError>
{
    let local_localstate_filename = context.get().borrow().localstate_file_path.clone();
    let path: &Path = Path::new(&local_localstate_filename);
    if ! path.is_file() {
        let msg = format!("broker_retrieve: UDT parameter error : {} is not a file", local_localstate_filename);
        log::error!("{}", msg);
        return Err(UdtError{err_code: 10001, err_msg: msg})
    }

    let client =  LocalstateBrokerClient::new(node_addr, context.get().borrow().broker_edge.port());

    info!("broker_retrieve: Retrieval data from : {:#?} into {}", client.node_syncer.get_ip(), local_localstate_filename);

    let client_connection = client.connect_client();

    match client_connection {
        Ok(connection) => {
    
            let mut buf: Box<Vec<u8>> = Box::new(vec![0u8]);

            match connection.recv(&mut buf) {

                Ok(size) => {

                    match fs::write(path, *buf) {
                        Ok(_) => {
                            info!("broker_retrieve: Successfully retrieved {} bytes of data from : {:#?} into {}", size, client.node_syncer.get_ip(), local_localstate_filename);
                            Ok(size)
                        }
                        Err(e) => {
                            error!("broker_retrieve: localstate retrieval error : {:#?}",  e);
                            Ok(0)
                        }
                    }
                },
                Err(e) => {
                    error!("broker_retrieve: localstate retrieval error : {:#?}",  e);
                    Err(e)
                }
            }
        },
        Err(e) => {

            error!("broker_retrieve: localstate retrieval error : {:#?}",  e);
            Err(e)
        }
    }
}

/// localstate data server, all the operation in a separate thread.
pub fn broker_server_start(
    context: ContextHandler<'static>,
    localstate_offset: u64,
) {
    let server = LocalstateBroker::new(context.clone());

    tokio::spawn(async move {
        debug!("broker_server_start: Server broker launched");
        match server.broker.listen() {
            Ok(_) => {
                info!("broker_server_start: Broker listening ....");
                loop {
                    info!("broker_server_start: awaiting connection...");
                    let connection = &mut match server.broker.accept() {
                        Ok(conn) => conn,
                        Err(e) => {
                            error!("broker_server_start: Broker accepting error : {:#?}", e);
                            continue;
                        }
                    };

                    info!("broker_server_start: accepted connection from {:?}", connection.get_name());
                    if let Ok(mut f) = File::open(context.get().borrow().localstate_file_path.clone()) {

                        if let Ok(_) = f.seek(SeekFrom::Start(localstate_offset)) {

                            if let Ok(metadata) = fs::metadata(&context.get().borrow().localstate_file_path) {

                                let mut buf = vec![0u8; metadata.len() as usize];
            
                                match f.read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(file_read_size) => {
                                        if let Err(e) = connection.send(&buf[0..file_read_size]) {
                                            error!("broker_server_start: Localstate file read error : {:#?}", e);
                                        }
                                    }
                                    Err(e) => {
                                        error!("broker_server_start: Localstate file read error : {}", e);
                                    }
                                }
                            }        
                        }
                    }
                }
                info!("broker_server_start: event loop exit.");
            },
            Err(e) => {
                error!("broker_server_start: Cannot launch listening on {:#?} with error : {:#?}", server.broker, e);
            }
        }
    });
}
