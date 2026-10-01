use super::*;
use std::sync::Weak;

pub(super) enum Event {
    Redirect(String),
    LoadFailure(String, i64),
}
pub(super) struct View {
    pub addr: std::net::SocketAddr,
    pub owner: Weak<FacebookOAuthSession>,
    pub events: Mutex<Vec<(FacebookLoginDialogRequest, Event)>>,
    pub dismissed: Mutex<Vec<(bool, FacebookSessionState)>>,
}
impl FacebookLoginDialogAdapter for View {
    fn show(&self, request: &FacebookLoginDialogRequest) -> Result<(), SocialPlatformError> {
        assert_eq!(
            self.owner.upgrade().unwrap().session_state(),
            FacebookSessionState::Opening
        );
        let path = request
            .authorization_url
            .strip_prefix(&format!("http://{}", self.addr))
            .unwrap();
        let mut stream =
            TcpStream::connect(self.addr).map_err(|_| SocialPlatformError::Transport)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            self.addr
        )
        .unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream
                .read_exact(&mut byte)
                .map_err(|_| SocialPlatformError::Transport)?;
            headers.push(byte[0]);
            assert!(headers.len() < 8192);
        }
        let headers = String::from_utf8(headers).unwrap();
        assert!(headers.starts_with("HTTP/1.1 200 "));
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert!(length < 16384);
        let mut body = vec![0; length];
        let event = match stream.read_exact(&mut body) {
            Ok(()) => Event::Redirect(String::from_utf8(body).unwrap()),
            // Actual premature EOF is a failed load, not a fabricated OAuth result.
            Err(error) => Event::LoadFailure(
                format!("std::io::{:?}", error.kind()),
                i64::from(error.raw_os_error().unwrap_or(0)),
            ),
        };
        self.events.lock().unwrap().push((request.clone(), event));
        Ok(())
    }
    fn dismiss(&self, _: &FacebookLoginDialogRequest, success: bool) {
        self.dismissed
            .lock()
            .unwrap()
            .push((success, self.owner.upgrade().unwrap().session_state()));
    }
}
