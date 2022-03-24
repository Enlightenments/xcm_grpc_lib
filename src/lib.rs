pub mod rpc;

#[derive(Debug, Default, Copy, Clone)]
struct MyGreeter {}

use tonic::{Request, Status, metadata::MetadataValue, Response};
use rpc::{RpcRequest, RpcResponse};
use rpc::rpc_server::{Rpc, RpcServer};
use std::net::SocketAddr;
use tonic::transport::{Server, Channel};
use rpc::rpc_client::RpcClient;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tokio::sync::mpsc;

use std::pin::Pin;
use futures::Stream;

#[tonic::async_trait]
impl Rpc for MyGreeter {
    async fn json_rpc(
        &self,
        request: Request<RpcRequest>,
    ) -> Result<Response<RpcResponse>, Status> {
        let req: RpcRequest = request.into_inner().clone();
        println!("single request {:?}", req);
        let rep: RpcResponse = RpcResponse {
            id: req.id,
            method: req.method,
            data: "3".to_string(),
        };
        Ok(Response::new(rep))
    }



    type SocketRpcStream = Pin<Box<dyn Stream<Item = Result<RpcResponse,Status>>
    + Send
    + 'static >>;

    async fn socket_rpc(&self, req: tonic::Request<tonic::Streaming<RpcRequest>>)->Result<Response<Self::SocketRpcStream>, Status>  {
        println!("stream_rpc run!");
        let mut in_stream = req.into_inner();
        let (tx, rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(v) => {
                        println!("stream .. {:?}",v);
                        tx.send(Ok(RpcResponse { id:v.id,method:v.method,data:v.params }))
                            .await
                            .expect("working rx")
                    },
                    Err(err) => {
                        println!("stream error{:?}",err);

                        match tx.send(Err(err)).await {
                            Ok(_) => (),
                            Err(_err) => break, // response was droped
                        }
                    }
                }
            }
            println!("\tstream ended");
        });

        // echo just write the same data that was received
        let out_stream = ReceiverStream::new(rx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::SocketRpcStream
        ))
    }
}

fn check_auth(req: Request<()>) -> Result<Request<()>, Status> {
    let _token = MetadataValue::from_str("token123456").unwrap();

    match req.metadata().get("token") {
        // Some(t) if token == t => Ok(req),
        // _ => Err(Status::unauthenticated("No valid auth token")),
        _ => Ok(req),
    }
}

pub async fn server(addr: SocketAddr) {
    let greeter = MyGreeter::default();
    let svc_json_rpc = RpcServer::with_interceptor(greeter, check_auth);
    println!("Starting gRPC Server...at {:?}", addr);
    if let Err(e) = Server::builder()
        .add_service(svc_json_rpc)
        .serve(addr)
        .await { println!("server error: {}", e); }
}

pub async fn client(rpc_req: RpcRequest, url: String) -> Result<RpcResponse, Box<dyn std::error::Error>> {
    let grpc_url: &'static str = string_to_static_str(url);
    let channel = Channel::from_static(grpc_url).connect().await?;
    let token = MetadataValue::from_str("token123456")?;
    let mut client = RpcClient::with_interceptor(channel, move |mut req: Request<()>| {
        req.metadata_mut().insert("token", token.clone());
        Ok(req)
    });
    let request = tonic::Request::new(rpc_req);
    let res = client.json_rpc(request).await?;
    Ok(res.into_inner())
}

fn string_to_static_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
