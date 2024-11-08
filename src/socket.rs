use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;

use std::path::Path;
use std::os::unix::net::UnixDatagram;


const MAX_UDP_PACKET_SIZE: usize = 65_535;


#[derive(Debug, Clone, Copy)]
pub enum ServerAction<T: Serialize> {
    Respond(T),
    #[allow(unused)]
    StopRespond(T),
    #[allow(unused)]
    Stop,
    #[allow(unused)]
    None,
}

pub trait ServerState {
    type Request<'de>: Deserialize<'de>;
    type Response: Serialize;

    fn update<'de>(&mut self, request: &Self::Request<'de>) -> ServerAction<Self::Response>;
}

pub fn start_server<S: ServerState>(path: &Path, mut state: S) -> std::io::Result<()> {
    let socket = UnixDatagram::bind(path)?;
    let mut buffer = vec![0u8; MAX_UDP_PACKET_SIZE];
    loop {
        let (size, sock_addr) = socket.recv_from(&mut buffer)?;
        let received_data = &buffer[..size];
        let request = bincode::deserialize(received_data).unwrap(); // TODO

        let action = state.update(&request);
        match action {
            ServerAction::Respond(ref response) | ServerAction::StopRespond(ref response) => {
                let response_data = bincode::serialize(&response).unwrap();
                socket.send_to_addr(&response_data, &sock_addr)?;

                if matches!(action, ServerAction::StopRespond(_)) {
                    break Ok(());
                }
            },
            ServerAction::Stop => break Ok(()),
            ServerAction::None => (),
        }
    }
}


// pub fn send(
//     client_sock_path: &Path,
//     server_sock_path: &Path,
//     request: &impl Serialize,
// ) -> std::io::Result<()> {
//     let msg = bincode::serialize(request).unwrap(); // TODO
//     let socket = UnixDatagram::bind(client_sock_path)?;
//     socket.send_to(&msg, server_sock_path)?;
//     Ok(())
// }

pub fn send_and_receive<Response: DeserializeOwned>(
    client_sock_path: impl AsRef<Path>,
    server_sock_path: impl AsRef<Path>,
    request: &impl Serialize,
) -> std::io::Result<Response> {
    let msg = bincode::serialize(request).unwrap();
    let socket = UnixDatagram::bind(client_sock_path.as_ref())?;
    socket.send_to(&msg, server_sock_path.as_ref())?;

    let mut buffer = vec![0u8; MAX_UDP_PACKET_SIZE];
    let size = socket.recv(&mut buffer)?;
    let response = bincode::deserialize(&buffer[..size]).unwrap();
    Ok(response)
}
