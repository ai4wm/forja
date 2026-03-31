use crate::dashboard::routes::build_router;
use forja_core::error::{ForjaError, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::task::JoinHandle;

pub mod routes;

#[cfg(test)]
mod tests;

pub struct DashboardServer {
    pub port: u16,
    pub db_path: PathBuf,
    pub handle: Option<JoinHandle<()>>,
}

impl DashboardServer {
    pub fn new(port: u16, db_path: PathBuf) -> Self {
        Self {
            port,
            db_path,
            handle: None,
        }
    }

    pub fn start(&mut self) -> Result<String> {
        let url = format!("http://localhost:{}", self.port);

        if let Some(handle) = &self.handle
            && !handle.is_finished() {
                open::that(&url).map_err(|error| ForjaError::Internal(error.to_string()))?;
                return Ok(url);
            }

        self.stop();

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port);
        let listener = TcpListener::bind(socket_addr)
            .map_err(|error| ForjaError::Internal(error.to_string()))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| ForjaError::Internal(error.to_string()))?;
        let listener = TokioTcpListener::from_std(listener)
            .map_err(|error| ForjaError::Internal(error.to_string()))?;
        let app = build_router(self.db_path.clone());

        self.handle = Some(tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, app).await {
                eprintln!("[Dashboard] server error: {error}");
            }
        }));

        open::that(&url).map_err(|error| ForjaError::Internal(error.to_string()))?;
        Ok(url)
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for DashboardServer {
    fn drop(&mut self) {
        self.stop();
    }
}
