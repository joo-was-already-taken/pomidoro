use crate::{Config, ConfirmationResponse, Request};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedReadHalf;

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to connect to server at {1:?}: {0}")]
    Connection(#[source] io::Error, String),
    #[error("Failed to send JSON request: {0}")]
    Send(#[source] io::Error),
    #[error("Failed to read server response: {0}")]
    Read(#[source] io::Error),
    #[error("Failed to parse server response: {0}")]
    Parse(#[source] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct Client {
    pub config: Config,
}

impl Client {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn connect(&self) -> Result<UnixStream, Error> {
        let sock_addr = self.config.socket.as_str();
        UnixStream::connect(sock_addr)
            .await
            .map_err(|e| Error::Connection(e, sock_addr.to_string()))
    }

    pub async fn send_request(
        &self,
        request: Request,
    ) -> Result<BufReader<OwnedReadHalf>, Error> {
        let stream = self.connect().await?;
        let (reader, mut writer) = stream.into_split();

        crate::send_json(&mut writer, &request)
            .await
            .map_err(Error::Send)?;

        Ok(BufReader::new(reader))
    }

    pub async fn send_and_confirm(
        &self,
        request: Request,
    ) -> Result<ConfirmationResponse, Error> {
        let mut reader = self.send_request(request).await?;
        let mut line = String::new();
        reader.read_line(&mut line).await.map_err(Error::Read)?;
        serde_json::from_str(&line).map_err(Error::Parse)
    }

    pub async fn start(&self) -> Result<ConfirmationResponse, Error> {
        self.send_and_confirm(Request::Start).await
    }

    pub async fn pause(&self) -> Result<ConfirmationResponse, Error> {
        self.send_and_confirm(Request::Pause).await
    }

    pub async fn resume(&self) -> Result<ConfirmationResponse, Error> {
        self.send_and_confirm(Request::Resume).await
    }

    pub async fn toggle(&self) -> Result<ConfirmationResponse, Error> {
        self.send_and_confirm(Request::Toggle).await
    }

    pub async fn stop(&self) -> Result<ConfirmationResponse, Error> {
        self.send_and_confirm(Request::Stop).await
    }

    pub async fn next_interval(&self) -> Result<ConfirmationResponse, Error> {
        self.send_and_confirm(Request::NextInterval).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Socket;
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn client_send_and_confirm() {
        let addr = "\0pomidoro-test-client-socket".to_string();
        let listener = UnixListener::bind(&addr).unwrap();

        let mut config = Config::parse("").unwrap();
        config.socket = Socket::Abstract(addr);

        let client = Client::new(config);

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            let (reader, mut writer) = stream.split();
            let mut buf_reader = tokio::io::BufReader::new(reader);

            buf_reader.read_line(&mut line).await.unwrap();
            assert!(line.contains("\"Start\""));

            let response = ConfirmationResponse {
                request: Request::Start,
                success: true,
                error_msg: String::new(),
            };
            crate::send_json(&mut writer, &response).await.unwrap();
        });

        let response = client.start().await.unwrap();
        assert!(response.success);
    }

    #[tokio::test]
    async fn client_all_commands() {
        let addr = "\0pomidoro-test-client-all-commands".to_string();
        let listener = UnixListener::bind(&addr).unwrap();

        let mut config = Config::parse("").unwrap();
        config.socket = Socket::Abstract(addr);

        let client = Client::new(config);

        tokio::spawn(async move {
            let expected_reqs = [
                "\"Pause\"",
                "\"Resume\"",
                "\"Toggle\"",
                "\"Stop\"",
                "\"NextInterval\"",
            ];

            for expected_req in expected_reqs {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                let (reader, mut writer) = stream.split();
                let mut buf_reader = tokio::io::BufReader::new(reader);

                buf_reader.read_line(&mut line).await.unwrap();
                assert!(line.contains(expected_req));

                let response = ConfirmationResponse {
                    request: Request::Start,
                    success: true,
                    error_msg: String::new(),
                };
                crate::send_json(&mut writer, &response).await.unwrap();
            }
        });

        assert!(client.pause().await.unwrap().success);
        assert!(client.resume().await.unwrap().success);
        assert!(client.toggle().await.unwrap().success);
        assert!(client.stop().await.unwrap().success);
        assert!(client.next_interval().await.unwrap().success);
    }
}
