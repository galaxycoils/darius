#[path = "request.rs"]
mod request;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

pub struct HttpFixture {
    pub url: String,
    pub requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl HttpFixture {
    // None holds a fully received request open until the binary's own deadline.
    pub fn start(response: Option<(u16, String)>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, quit) = (requests.clone(), stop.clone());
        let join = std::thread::spawn(move || {
            let mut held = Vec::new();
            while !quit.load(Ordering::SeqCst) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                request::receive(&mut socket);
                seen.fetch_add(1, Ordering::SeqCst);
                if let Some((status, body)) = &response {
                    let _ = write!(
                        socket,
                        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                } else {
                    held.push(socket);
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            join: Some(join),
        }
    }
}
impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.join.take().unwrap().join().unwrap();
    }
}
