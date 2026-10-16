//! A local HTTP/1.1 server that stands in for a provider: it answers
//! scripted replies in order, one connection each, and records every request.
//! No network beyond the loopback, no timing.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

/// A request the server got.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    /// The path with its query.
    pub path: String,
    /// Header names in lower case, in order.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap()
    }
}

/// The two sides of a held reply: the first part sent, the test releasing
/// the rest, and the client closing the connection.
#[derive(Debug)]
struct HeldState {
    sent: Semaphore,
    released: Semaphore,
    closed: Semaphore,
}

/// Lets a test see a held reply start, then release it or see the client
/// drop it.
#[derive(Debug, Clone)]
pub struct Held(Arc<HeldState>);

impl Default for Held {
    fn default() -> Self {
        Self(Arc::new(HeldState {
            sent: Semaphore::new(0),
            released: Semaphore::new(0),
            closed: Semaphore::new(0),
        }))
    }
}

impl Held {
    /// Waits until the head and the first part of the body are sent.
    pub async fn sent(&self) {
        self.0.sent.acquire().await.unwrap().forget();
    }

    /// Lets the server send the rest of the reply.
    #[allow(dead_code)]
    pub fn release(&self) {
        self.0.released.add_permits(1);
    }

    /// Waits until the client has closed the connection.
    pub async fn closed(&self) {
        self.0.closed.acquire().await.unwrap().forget();
    }
}

/// One scripted reply.
pub enum Reply {
    /// A whole reply.
    Full {
        status: u16,
        content_type: &'static str,
        body: String,
    },
    /// A reply whose body is sent in two parts, the second only once the
    /// test releases it. Its length is not given: the body ends when the
    /// server closes the connection.
    Held {
        content_type: &'static str,
        first: String,
        rest: String,
        held: Held,
    },
}

impl Reply {
    pub fn json(status: u16, body: impl Into<String>) -> Self {
        Self::Full {
            status,
            content_type: "application/json",
            body: body.into(),
        }
    }

    pub fn events(body: impl Into<String>) -> Self {
        Self::Full {
            status: 200,
            content_type: "text/event-stream",
            body: body.into(),
        }
    }
}

/// The server: its base URL and the requests it got.
pub struct FakeServer {
    pub url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl FakeServer {
    /// Starts a server on a free loopback port that answers `replies` in
    /// order and then stops accepting connections.
    pub async fn start(replies: impl IntoIterator<Item = Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut replies: VecDeque<Reply> = replies.into_iter().collect();
        tokio::spawn({
            let requests = Arc::clone(&requests);
            async move {
                while let Some(reply) = replies.pop_front() {
                    let Ok((stream, _)) = listener.accept().await else {
                        return;
                    };
                    serve(stream, reply, &requests).await;
                }
            }
        });
        Self { url, requests }
    }

    /// Every request the server got, in order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Other",
    }
}

async fn serve(mut stream: TcpStream, reply: Reply, requests: &Mutex<Vec<Recorded>>) {
    let Some(request) = read_request(&mut stream).await else {
        return;
    };
    requests.lock().unwrap().push(request);
    match reply {
        Reply::Full {
            status,
            content_type,
            body,
        } => {
            let head = format!(
                "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n",
                reason(status),
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        }
        Reply::Held {
            content_type,
            first,
            rest,
            held,
        } => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(first.as_bytes()).await;
            let _ = stream.flush().await;
            held.0.sent.add_permits(1);
            let mut byte = [0_u8; 1];
            tokio::select! {
                permit = held.0.released.acquire() => {
                    permit.unwrap().forget();
                    let _ = stream.write_all(rest.as_bytes()).await;
                }
                // The client sends nothing more, so a read ends only when it
                // closes the connection.
                _ = stream.read(&mut byte) => held.0.closed.add_permits(1),
            }
        }
    }
    let _ = stream.shutdown().await;
}

/// Reads one request: the head up to the blank line, then `Content-Length`
/// bytes of body.
async fn read_request(stream: &mut TcpStream) -> Option<Recorded> {
    let mut data = Vec::new();
    let mut buffer = [0_u8; 4096];
    let head_end = loop {
        if let Some(at) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break at;
        }
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        data.extend_from_slice(&buffer[..read]);
    };
    let head = String::from_utf8(data[..head_end].to_vec()).ok()?;
    let mut lines = head.split("\r\n");
    let mut start = lines.next()?.split(' ');
    let method = start.next()?.to_owned();
    let path = start.next()?.to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    let mut body = data[head_end + 4..].to_vec();
    while body.len() < length {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        body.extend_from_slice(&buffer[..read]);
    }
    Some(Recorded {
        method,
        path,
        headers,
        body: String::from_utf8(body).ok()?,
    })
}

/// A base URL nothing listens on: a port that was free a moment ago.
pub async fn closed_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}
