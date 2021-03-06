use anyhow::Result;
use hyper::{
    client::{connect::Connect, HttpConnector},
    Uri,
};
use std::str::FromStr;

use crate::{transport::Transport, Request, Response};

const USER_AGENT: &str = concat!("jason.rs/", std::env!("CARGO_PKG_VERSION"));

/// HTTP client.
#[derive(Debug, Clone)]
pub struct Client {
    raw: RawClient<HttpConnector>,
}

impl Client {
    /// Creates a new HTTP client connected to the server at the given URL.
    pub fn new(addr: &str) -> Result<Self> {
        Ok(Client {
            raw: RawClient::new(addr, HttpConnector::new())?,
        })
    }
}

impl Transport for Client {
    fn request(
        &self,
        req: Request,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = Result<Response>> + Send + '_>> {
        self.raw.request(req)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RawClient<C> {
    pub(crate) uri: Uri,
    pub(crate) http_client: hyper::Client<C>,
}

impl<C: Connect + Clone> RawClient<C> {
    pub(crate) fn new(addr: &str, connector: C) -> Result<Self> {
        Ok(Self {
            uri: Uri::from_str(addr)?,
            http_client: hyper::Client::builder().build(connector),
        })
    }
}

impl<C: Connect + Clone + Send + Sync + 'static> Transport for RawClient<C> {
    fn request(
        &self,
        req: Request,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = Result<Response>> + Send + '_>> {
        Box::pin(async move {
            let req_uri = self.uri.clone();
            let req_body = serde_json::to_string(&req)?;

            let http_req = hyper::Request::builder()
                .header(hyper::header::USER_AGENT, USER_AGENT)
                .header(hyper::header::CONTENT_TYPE, "application/json")
                .method(hyper::Method::POST)
                .uri(req_uri)
                .body(hyper::Body::from(req_body))?;

            let res_body = self.http_client.request(http_req).await?.into_body();

            let res_data = hyper::body::to_bytes(res_body).await?;

            let parsed_res: Response = serde_json::from_slice(&res_data)?;

            Ok(parsed_res)
        })
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use crate::{ProtocolVersion, Request, RequestId, ResultRes};

    use super::*;

    use std::convert::Infallible;

    async fn test_server_handle(
        _req: hyper::Request<hyper::Body>,
    ) -> Result<hyper::Response<hyper::Body>, Infallible> {
        Ok::<_, Infallible>(hyper::Response::new(hyper::Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": "1",
                "result": 7,
            })
            .to_string(),
        )))
    }

    async fn start_jsonrpc_test_server() {
        let server = hyper::Server::bind(&std::net::SocketAddr::from(([127, 0, 0, 1], 3000)));

        let make_service = hyper::service::make_service_fn(|_conn| async move {
            Ok::<_, Infallible>(hyper::service::service_fn(test_server_handle))
        });

        tokio::spawn(server.serve(make_service));
    }

    #[tokio::test]
    async fn it_works() {
        start_jsonrpc_test_server().await;

        let c = Client::new("http://127.0.0.1:3000").expect("failed to create client");

        let res: Response = c
            .request(Request {
                jsonrpc: ProtocolVersion::TwoPointO,
                id: RequestId::String("1".to_string()),
                method: "some_method".to_string(),
                params: None,
            })
            .await
            .expect("test request failed");

        assert_eq!(
            res,
            Response(Ok(ResultRes {
                jsonrpc: ProtocolVersion::TwoPointO,
                id: RequestId::String("1".to_string()),
                result: json!(7),
            }))
        );
    }
}
