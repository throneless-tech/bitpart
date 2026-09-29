// Bitpart
// Copyright (C) 2025 Throneless Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use axum::Router;
use axum_server::{Handle, tls_rustls::RustlsConfig};
use bitpart_common::error::{BitpartErrorKind, Result};
use std::net::SocketAddr;
use std::path::PathBuf;

/// PEM-encoded certificate chain and private key used to serve the client API over TLS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsFiles {
    pub cert: PathBuf,
    pub key: PathBuf,
}

impl TlsFiles {
    /// Build the TLS settings from the `tls_cert` / `tls_key` configuration values.
    ///
    /// Both must be given to enable TLS; giving only one of them is a configuration error
    /// rather than a silent fallback to cleartext.
    pub fn from_config(cert: Option<&str>, key: Option<&str>) -> Result<Option<Self>> {
        match (cert, key) {
            (Some(cert), Some(key)) => Ok(Some(Self {
                cert: cert.into(),
                key: key.into(),
            })),
            (None, None) => Ok(None),
            (Some(_), None) => Err(BitpartErrorKind::Tls(
                "a TLS certificate was given without a private key (set tls_key)".to_owned(),
            )
            .into()),
            (None, Some(_)) => Err(BitpartErrorKind::Tls(
                "a TLS private key was given without a certificate (set tls_cert)".to_owned(),
            )
            .into()),
        }
    }

    /// Read and validate the certificate and key.
    pub async fn load(&self) -> Result<RustlsConfig> {
        // Bitpart only enables rustls' `ring` backend, so make it the process default
        // explicitly instead of relying on crate feature unification to pick one.
        let _ = rustls::crypto::ring::default_provider().install_default();

        RustlsConfig::from_pem_file(&self.cert, &self.key)
            .await
            .map_err(|err| {
                BitpartErrorKind::Tls(format!(
                    "failed to load TLS certificate `{}` and key `{}`: {err}",
                    self.cert.display(),
                    self.key.display()
                ))
                .into()
            })
    }
}

/// Whether the client API would be reachable from other hosts without TLS, i.e. the
/// authorization token and all API traffic would cross the network in cleartext.
pub fn exposed_without_tls(addr: &SocketAddr, tls: bool) -> bool {
    !tls && !addr.ip().is_loopback()
}

/// Serve `app` over TLS on `addr` until `handle` is shut down.
pub async fn serve(
    addr: SocketAddr,
    config: RustlsConfig,
    app: Router,
    handle: Handle<SocketAddr>,
) -> Result<()> {
    axum_server::bind_rustls(addr, config)
        .handle(handle)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authenticate;
    use crate::socket;
    use crate::utils::test_state;
    use axum::{middleware, routing::any};
    use futures::{SinkExt, StreamExt};
    use std::sync::Arc;
    use tokio_tungstenite::{
        Connector, connect_async, connect_async_tls_with_config,
        tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
    };

    struct TestCert {
        _dir: tempfile::TempDir,
        files: TlsFiles,
        cert_der: Vec<u8>,
    }

    fn self_signed() -> TestCert {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let files = TlsFiles {
            cert: dir.path().join("cert.pem"),
            key: dir.path().join("key.pem"),
        };
        std::fs::write(&files.cert, cert.serialize_pem().unwrap()).unwrap();
        std::fs::write(&files.key, cert.serialize_private_key_pem()).unwrap();
        TestCert {
            _dir: dir,
            files,
            cert_der: cert.serialize_der().unwrap(),
        }
    }

    fn trusting(cert_der: &[u8]) -> Connector {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der.to_vec().into()).unwrap();
        Connector::Rustls(Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        ))
    }

    /// Start the real client API (websocket route behind the auth middleware) over TLS.
    async fn start_tls_server(
        files: &TlsFiles,
    ) -> (SocketAddr, Handle<SocketAddr>, tempfile::TempDir) {
        let (state, dir) = test_state().await;
        let app = Router::new()
            .route("/ws", any(socket::handler))
            .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
            .with_state(state);
        let config = files.load().await.unwrap();
        let handle = Handle::new();
        tokio::spawn(serve(
            "127.0.0.1:0".parse().unwrap(),
            config,
            app,
            handle.clone(),
        ));
        let addr = handle.listening().await.expect("server failed to start");
        (addr, handle, dir)
    }

    fn request(
        url: &str,
        auth: &str,
    ) -> tokio_tungstenite::tungstenite::handshake::client::Request {
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Authorization", HeaderValue::from_str(auth).unwrap());
        request
    }

    #[test]
    fn tls_requires_both_cert_and_key() {
        assert_eq!(TlsFiles::from_config(None, None).unwrap(), None);
        assert_eq!(
            TlsFiles::from_config(Some("cert.pem"), Some("key.pem")).unwrap(),
            Some(TlsFiles {
                cert: "cert.pem".into(),
                key: "key.pem".into()
            })
        );
        assert!(TlsFiles::from_config(Some("cert.pem"), None).is_err());
        assert!(TlsFiles::from_config(None, Some("key.pem")).is_err());
    }

    #[test]
    fn warns_only_for_non_loopback_cleartext() {
        for addr in [
            "0.0.0.0:3000",
            "[::]:3000",
            "192.168.1.10:3000",
            "10.0.0.2:80",
        ] {
            let addr: SocketAddr = addr.parse().unwrap();
            assert!(exposed_without_tls(&addr, false), "{addr}");
            assert!(!exposed_without_tls(&addr, true), "{addr}");
        }
        for addr in ["127.0.0.1:3000", "127.1.2.3:3000", "[::1]:3000"] {
            let addr: SocketAddr = addr.parse().unwrap();
            assert!(!exposed_without_tls(&addr, false), "{addr}");
        }
    }

    #[tokio::test]
    async fn load_rejects_missing_or_invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        let missing = TlsFiles {
            cert: dir.path().join("missing-cert.pem"),
            key: dir.path().join("missing-key.pem"),
        };
        assert!(missing.load().await.is_err());

        let garbage = TlsFiles {
            cert: dir.path().join("cert.pem"),
            key: dir.path().join("key.pem"),
        };
        std::fs::write(&garbage.cert, "not a certificate").unwrap();
        std::fs::write(&garbage.key, "not a key").unwrap();
        assert!(garbage.load().await.is_err());

        // A valid certificate paired with a key that is not a private key.
        let cert = self_signed();
        let mismatched = TlsFiles {
            cert: cert.files.cert.clone(),
            key: cert.files.cert.clone(),
        };
        assert!(mismatched.load().await.is_err());
        assert!(cert.files.load().await.is_ok());
    }

    #[tokio::test]
    async fn api_is_served_over_tls() {
        let cert = self_signed();
        let (addr, handle, _db) = start_tls_server(&cert.files).await;
        let url = format!("wss://localhost:{}/ws", addr.port());

        let (mut ws, _) = connect_async_tls_with_config(
            request(&url, "test"),
            None,
            false,
            Some(trusting(&cert.cert_der)),
        )
        .await
        .expect("TLS websocket handshake");
        ws.send(Message::text(r#"{ "message_type": "ListBots" }"#))
            .await
            .unwrap();
        let reply = ws.next().await.unwrap().unwrap();
        let reply: serde_json::Value = serde_json::from_str(reply.to_text().unwrap()).unwrap();
        assert_eq!(reply["message_type"], "Response");
        assert_eq!(reply["data"]["response_type"], "ListBots");

        handle.shutdown();
    }

    #[tokio::test]
    async fn tls_server_still_requires_auth() {
        let cert = self_signed();
        let (addr, handle, _db) = start_tls_server(&cert.files).await;
        let url = format!("wss://localhost:{}/ws", addr.port());

        let err = connect_async_tls_with_config(
            request(&url, "wrong"),
            None,
            false,
            Some(trusting(&cert.cert_der)),
        )
        .await
        .expect_err("wrong token must be rejected");
        assert!(err.to_string().contains("401"), "{err}");

        handle.shutdown();
    }

    #[tokio::test]
    async fn tls_server_rejects_cleartext_and_untrusted_clients() {
        let cert = self_signed();
        let (addr, handle, _db) = start_tls_server(&cert.files).await;

        // A cleartext client never gets as far as sending its token in a readable request.
        let cleartext = format!("ws://localhost:{}/ws", addr.port());
        assert!(connect_async(request(&cleartext, "test")).await.is_err());

        // A client that does not trust the server certificate refuses to connect.
        let other = self_signed();
        let url = format!("wss://localhost:{}/ws", addr.port());
        assert!(
            connect_async_tls_with_config(
                request(&url, "test"),
                None,
                false,
                Some(trusting(&other.cert_der)),
            )
            .await
            .is_err()
        );

        handle.shutdown();
    }
}
