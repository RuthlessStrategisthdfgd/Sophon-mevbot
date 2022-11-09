use std::net::{SocketAddr, IpAddr, AddrParseError};
use telemetry::{error, debug};
use udt::{UdtSocket, UdtError, UdtOpts, SocketType, SocketFamily};

use crate::error::DataBrokerError;

const BUFFER_SIZE: i32 = 4096000;

/// 
/// Initialize UDT Socket
/// 
fn init_udt_socket() -> UdtSocket {
    udt::init();
    let udt_socket = match UdtSocket::new(SocketFamily::AFInet, SocketType::Stream) {
        Ok(sock) => sock,
        Err(e) => {
            error!("init_udt_socket: Udt creation error : {:#?}", e);
            todo!();
        }
    };
    if let Err(e) = udt_socket.setsockopt(UdtOpts::UDP_RCVBUF, BUFFER_SIZE) {
        error!("init_udt_socket: Setting option UDP_RCVBUF: {:#?}", e);
    }
    if let Err(e) = udt_socket.setsockopt(UdtOpts::UDP_SNDBUF, BUFFER_SIZE) {
        error!("init_udt_socket: Setting option UDP_SNDBUF: {:#?}", e);
    }
    udt_socket
}

///
/// Initialize socket
/// 
/// # Arguments
/// * `server_ip`           - Server IP address
/// * `server_port`         - Server Port
/// 
fn init_socket_addr(server_ip: String, server_port: u16) -> Result<SocketAddr, AddrParseError> {
    let connection_string = format!("{}:{}", server_ip, server_port);
    match connection_string.parse::<SocketAddr>() {
        Ok(socket) => Ok(socket),
        Err(e) => {
            error!("Client connection error to {} : {}", connection_string, e);
            Err(e)
        }
    }
}

trait UDTransfer {
    fn send_udt_data(&self, buf: &[u8]) -> Result<usize, UdtError>;
    fn recv_udt_data(&self, buf: &mut Box<Vec<u8>>) -> Result<usize, UdtError>;
}

impl UDTransfer for UdtSocket {

    ///
    /// Transfer data to a remote data server
    /// 
    /// # Arguments
    /// * `buf`           - Buffer with data
    /// 
    fn send_udt_data(&self, buf: &[u8]) -> Result<usize, UdtError> {
        let mut total_size: usize = 0;
        let buf_size: [u8; 8] = buf.len().to_be_bytes();
        match self.send(&buf_size) {
            Ok(8) => {
                debug!("Received correct transfer format : {} Bytes to be transferred.", buf.len());
                while total_size < buf.len() {
                    match self.send(&buf[total_size..]) {
                        Ok(delta) => {
                            total_size +=  delta as usize;
                            debug!("Sent {} Bytes and {} Bytes total.", delta, total_size);
                        },
                        Err(e) => {
                            error!("UDT Data sending error : {:#?}", e);
                            break;
                        }
                    }
                }
                debug!("Transfer complete, {} Bytes was transferred.", total_size);
                Ok(total_size)
            }
            Ok(size) =>  {
                let msg = format!("UDT protocol error, wrong size : {}", size);
                error!("{}", msg);
                Err(UdtError{err_code: 10000, err_msg: msg})
            }
            Err(e) => {
                error!("UDT Data sending error : {:#?}", e);
                Err(e)
            }
        }
    }

    ///
    /// Retrieve data from a remote data server
    /// 
    /// # Arguments
    /// * `buf`           - Buffer with data
    /// 
    fn recv_udt_data(&self, buf: &mut Box<Vec<u8>>) -> Result<usize, UdtError> {
        let mut total_size: usize = 0;
        let mut buf_size: [u8; 8] = [0u8; 8];
        match self.recv(&mut buf_size, 8) {
            Ok(8) => {
                let len = usize::from_be_bytes(buf_size);
                debug!("Received correct transfer format : {} Bytes to be received.", len);
                if buf.len() < len  {
                    buf.resize(len, 0u8);
                }
                while total_size < len {
                    let remaining = len - total_size;
                    match self.recv(&mut buf[total_size..], remaining) {
                        Ok(delta) => {
                            total_size += delta as usize;
                            debug!("Received {} Bytes and {} Bytes total.", delta, total_size);
                        },
                        Err(e) => {
                            error!("UDT Data receiving error : {:#?}", e);
                            break;
                        }
                    }
                }
                debug!("Transfer complete, {} Bytes was received.", total_size);
                Ok(total_size)        
            }
            Ok(size) =>  {
                let msg = format!("UDT protocol error, wrong size : {}", size);
                error!("{}", msg);
                Err(UdtError{err_code: 10000, err_msg: msg})
            }
            Err(e) => {
                error!("UDT Data receiving error : {:#?}", e);
                Err(e)
            }
        }
    }

}

///
/// Data server struct, represents its  internal details
/// 
#[derive(Debug)]
pub struct DATAServer {
    pub ip_addr: IpAddr,
    pub port: u16,
    sock: UdtSocket,
}

///
/// Data client struct, represents its  internal details
/// 
pub struct DATAClient {
    addr: SocketAddr,
    sock: UdtSocket,
}

/// 
/// Data server connection.
/// 
pub struct DATAServerConnection {
    sock: UdtSocket,
}

/// 
/// Data client connection.
/// 
pub struct DATAClientConnection {
    sock: UdtSocket,
}

impl DATAClient {

    ///
    /// Builds a new data client for data transfer.
    /// 
    /// # Arguments
    /// * `remote_ip`           - Remote server IP, as String
    /// * `remote_port`         - Remote server PORT
    /// 
    pub fn _new(remote_ip: String, remote_port: u16) -> Result<DATAClient, DataBrokerError> {
        let sock = init_udt_socket();
        match init_socket_addr(remote_ip, remote_port) {
            Ok(addr) => Ok(DATAClient {
                                        addr,
                                        sock,
                                    }),
            Err(_) => Err(DataBrokerError::ConnectionError)
        }
    }

    ///
    /// Builds a new data client for data transfer.
    /// 
    /// # Arguments
    /// * `remote_ip`           - Remote server IP, as IpAddr
    /// * `remote_port`         - Remote server PORT
    /// 
    pub fn new_from_ip_addr(remote_ip: IpAddr, remote_port: u16) -> DATAClient {
        let sock = init_udt_socket();
        let connection_string = format!("{}:{}", remote_ip.to_string(), remote_port);
        let addr = match connection_string.parse::<SocketAddr>() {
            Ok(addr) => addr,
            Err(e) => {
                error!("IpAddr parse error : {:#?}", e);
                format!("127.0.0.1:0").parse::<SocketAddr>().unwrap()
            }
        };

        DATAClient {
            addr,
            sock,
        }
    }

    ///
    /// Try to connect to the remote server.
    /// 
    pub fn connect(&self) -> Result<DATAClientConnection, UdtError> {
        debug!("Connecting to : {:#?} ...", self.addr);
        match self.sock.connect(self.addr) {
            Ok(_) => 
                Ok(DATAClientConnection {
                    sock: self.sock,
                }),
            Err(e) => Err(e)
        }
    }

    pub fn get_ip(&self) -> &SocketAddr {
        &self.addr
    }
}

impl DATAServer {

    ///
    /// Builds a new data server for data transfer.
    /// 
    /// # Arguments
    /// * `ip_addr`         - local server IP, as String
    /// * `port`            - local server PORT
    /// 
    pub fn new(ip_addr: IpAddr, port: u16) -> DATAServer {
        let sock = init_udt_socket();
        let sock_addr = SocketAddr::new(ip_addr, port);
        if let Err(e) = sock.bind(sock_addr) {
            error!("DATAServer::new, binding error : {:#?}", e);
        }
        DATAServer {
            sock,
            ip_addr,
            port,
        }
    }
    
    ///
    /// Try to listen on the predefined address and port
    /// 
    pub fn listen(&self) -> Result<(), UdtError> {
        self.sock.listen(1)
    }

    /// 
    /// Accepts a new incoming connection.
    /// 
    pub fn accept(&self) -> Result<DATAServerConnection, UdtError> {
        self.sock.accept().map(move |(sock, _)| {
            DATAServerConnection {
                sock,
            }
        })
    }
}

impl DATAServerConnection {

    /// 
    /// Retrieves address of the remote client
    /// 
    pub fn get_name(&self) -> Result<SocketAddr, UdtError> {
        self.sock.getpeername()
    }

    ///
    /// Sends data to a remote data client
    /// 
    /// # Arguments
    /// * `buffer`           - Buffer with data
    /// 
    pub fn send(&self, buffer: &[u8]) -> Result<usize, UdtError> {
        self.sock.send_udt_data(buffer)
    }

    ///
    /// Retrieves data from a remote data client
    /// 
    /// # Arguments
    /// * `buffer`           - Buffer with data
    /// 
    pub fn _recv(&self, buffer:  &mut Box<Vec<u8>>) -> Result<usize, UdtError> {
        self.sock.recv_udt_data(buffer)
    }
}

impl DATAClientConnection {
 
    ///
    /// Sends data to a remote data server
    /// 
    /// # Arguments
    /// * `buffer`           - Buffer with data
    /// 
    pub fn _send(&self, buffer: &[u8]) -> Result<usize, UdtError> {
        self.sock.send_udt_data(buffer)
    }

    ///
    /// Retrieves data from a remote data server
    /// 
    /// # Arguments
    /// * `buffer`           - Buffer with data
    /// 
    pub fn recv(&self, buffer:  &mut Box<Vec<u8>>) -> Result<usize, UdtError> {
        self.sock.recv_udt_data(buffer)
    }
}
