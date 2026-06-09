use detta_rpc::{RpcErrorBody, RpcRequest, RpcResponse, RpcResult};
use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::str;

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    Rpc(RpcErrorBody),
    EmptyResponse,
    InvalidHttpResponse(String),
    UnexpectedResult {
        expected: &'static str,
        actual: RpcResult,
    },
    UnexpectedSuccess(RpcResult),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Encode(error) => write!(f, "JSON encode error: {error}"),
            Self::Decode(error) => write!(f, "JSON decode error: {error}"),
            Self::Rpc(error) => write!(f, "RPC error {}: {}", error.code, error.message),
            Self::EmptyResponse => write!(f, "empty RPC response"),
            Self::InvalidHttpResponse(error) => write!(f, "invalid HTTP response: {error}"),
            Self::UnexpectedResult { expected, actual } => {
                write!(f, "expected {expected}, got {actual:?}")
            }
            Self::UnexpectedSuccess(result) => write!(f, "expected RPC error, got {result:?}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<io::Error> for ClientError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(error)
    }
}

pub type ClientResult<T> = Result<T, ClientError>;

pub struct TcpRpcClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl TcpRpcClient {
    pub fn connect(addr: SocketAddr) -> ClientResult<Self> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader })
    }

    pub fn request(&mut self, request: RpcRequest) -> ClientResult<RpcResponse> {
        serde_json::to_writer(&mut self.stream, &request).map_err(ClientError::Encode)?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()?;
        self.read_response()
    }

    pub fn raw_line(&mut self, line: &[u8]) -> ClientResult<RpcResponse> {
        self.stream.write_all(line)?;
        if !line.ends_with(b"\n") {
            self.stream.write_all(b"\n")?;
        }
        self.stream.flush()?;
        self.read_response()
    }

    fn read_response(&mut self) -> ClientResult<RpcResponse> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        if line.is_empty() {
            return Err(ClientError::EmptyResponse);
        }
        serde_json::from_str(&line).map_err(ClientError::Decode)
    }

    pub fn ok(&mut self, request: RpcRequest) -> ClientResult<RpcResult> {
        match self.request(request)? {
            RpcResponse::Ok(result) => Ok(result),
            RpcResponse::Error(error) => Err(ClientError::Rpc(error)),
        }
    }

    pub fn error(&mut self, request: RpcRequest) -> ClientResult<RpcErrorBody> {
        match self.request(request)? {
            RpcResponse::Error(error) => Ok(error),
            RpcResponse::Ok(result) => Err(ClientError::UnexpectedSuccess(result)),
        }
    }

    pub fn close(self) {
        let Self { stream, reader } = self;
        let reader_stream = reader.into_inner();
        let _ = stream.shutdown(Shutdown::Both);
        let _ = reader_stream.shutdown(Shutdown::Both);
    }
}

pub struct HttpRpcClient {
    addr: SocketAddr,
}

impl HttpRpcClient {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }

    pub fn post(&self, request: RpcRequest) -> ClientResult<(u16, RpcResponse)> {
        let body = serde_json::to_vec(&request).map_err(ClientError::Encode)?;
        self.post_bytes(&body)
    }

    pub fn post_bytes(&self, body: &[u8]) -> ClientResult<(u16, RpcResponse)> {
        let headers = format!(
            "POST /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let mut request = headers.into_bytes();
        request.extend(body);
        self.raw_request(&request)
    }

    pub fn raw_request(&self, request: &[u8]) -> ClientResult<(u16, RpcResponse)> {
        let mut stream = TcpStream::connect(self.addr)?;
        stream.write_all(request)?;
        stream.flush()?;
        stream.shutdown(Shutdown::Write)?;

        let mut response_bytes = Vec::new();
        stream.read_to_end(&mut response_bytes)?;
        parse_http_rpc_response(&response_bytes)
    }
}

fn parse_http_rpc_response(response: &[u8]) -> ClientResult<(u16, RpcResponse)> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| ClientError::InvalidHttpResponse("missing header terminator".into()))?;
    let headers = str::from_utf8(&response[..split])
        .map_err(|error| ClientError::InvalidHttpResponse(error.to_string()))?;
    let body = &response[split + 4..];
    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| ClientError::InvalidHttpResponse("missing status line".into()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| ClientError::InvalidHttpResponse("missing status code".into()))?
        .parse::<u16>()
        .map_err(|error| ClientError::InvalidHttpResponse(error.to_string()))?;
    let content_length = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .ok_or_else(|| ClientError::InvalidHttpResponse("missing content-length".into()))?
        .1
        .trim()
        .parse::<usize>()
        .map_err(|error| ClientError::InvalidHttpResponse(error.to_string()))?;
    if body.len() != content_length {
        return Err(ClientError::InvalidHttpResponse(format!(
            "body length {} did not match content-length {}",
            body.len(),
            content_length
        )));
    }
    serde_json::from_slice(body)
        .map(|response| (status, response))
        .map_err(ClientError::Decode)
}
