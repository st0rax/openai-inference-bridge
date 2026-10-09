//! Independent Chromium/Edge DevTools driver using only the Rust standard library.
//!
//! The debugging endpoint is bound to loopback and the browser profile is locked
//! for the lifetime of the driver. This module does not reuse WebAgent code.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::brain_backend::{
    BackendError, BackendErrorKind, BrowserPageDriver, CancellationToken, Readiness, TextSnapshot,
};
use crate::json::{self, JsonValue};

const MAX_HTTP_HEAD: usize = 16 * 1024;
const MAX_DEBUG_HTTP_BODY: usize = 1024 * 1024;
const MAX_WEBSOCKET_FRAME: usize = 8 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
static MASK_COUNTER: AtomicU32 = AtomicU32::new(0);

pub struct BrowserCdpDriver {
    executable_override: Option<PathBuf>,
    child: Option<Child>,
    cdp: Option<CdpClient>,
    profile_lock_path: Option<PathBuf>,
    _profile_lock: Option<File>,
    baseline_count: usize,
    baseline_last_text: String,
    last_candidate: String,
    stable_samples: usize,
}

impl BrowserCdpDriver {
    pub fn new() -> Self {
        let executable_override = env::var_os("OIB_BROWSER_EXECUTABLE")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Self {
            executable_override,
            child: None,
            cdp: None,
            profile_lock_path: None,
            _profile_lock: None,
            baseline_count: 0,
            baseline_last_text: String::new(),
            last_candidate: String::new(),
            stable_samples: 0,
        }
    }

    fn launch_browser(&self, profile_dir: &Path, port: u16) -> Result<Child, BackendError> {
        let profile_arg = format!("--user-data-dir={}", profile_dir.display());
        let port_arg = format!("--remote-debugging-port={port}");
        let args = [
            "--remote-debugging-address=127.0.0.1",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-mode",
            &port_arg,
            &profile_arg,
            "about:blank",
        ];

        if let Some(executable) = &self.executable_override {
            return Command::new(executable)
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| browser_unavailable("configured browser executable could not start"));
        }

        for executable in default_browser_candidates() {
            if let Ok(child) = Command::new(executable)
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                return Ok(child);
            }
        }
        Err(browser_unavailable(
            "no supported Chromium or Edge executable was found; set OIB_BROWSER_EXECUTABLE",
        ))
    }

    fn wait_for_debugger(&mut self, port: u16, deadline: Instant) -> Result<(), BackendError> {
        loop {
            ensure_before_deadline(deadline)?;
            if let Some(child) = self.child.as_mut() {
                if child.try_wait().map_err(|_| {
                    browser_unavailable("browser process status could not be checked")
                })?.is_some()
                {
                    return Err(browser_unavailable("browser exited before DevTools became ready"));
                }
            }

            let attempt_deadline = (Instant::now() + Duration::from_millis(250)).min(deadline);
            if http_json(port, "GET", "/json/version", attempt_deadline).is_ok() {
                return Ok(());
            }
            sleep_until(deadline, Duration::from_millis(50))?;
        }
    }

    fn evaluate_json(&mut self, expression: &str, deadline: Instant) -> Result<JsonValue, BackendError> {
        self.cdp
            .as_mut()
            .ok_or_else(|| browser_unavailable("DevTools connection is not open"))?
            .evaluate_json(expression, deadline)
    }

    fn page_state(&mut self, deadline: Instant) -> Result<JsonValue, BackendError> {
        self.evaluate_json(
            r#"JSON.stringify((()=>{const n=Array.from(document.querySelectorAll('[data-message-author-role="assistant"]'));const e=document.querySelector('#prompt-textarea,[contenteditable="true"]');return {url:location.href,body:(document.body?.innerText||'').slice(0,4000),title:document.title,editor:!!e,count:n.length,last:n.length?n[n.length-1].innerText:''};})())"#,
            deadline,
        )
    }

    fn check_browser_alive(&mut self) -> Result<(), BackendError> {
        let Some(child) = self.child.as_mut() else {
            return Err(browser_unavailable("browser process has not been started"));
        };
        if child
            .try_wait()
            .map_err(|_| browser_unavailable("browser process status could not be checked"))?
            .is_some()
        {
            return Err(browser_unavailable("browser process exited"));
        }
        Ok(())
    }
}

impl Default for BrowserCdpDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserPageDriver for BrowserCdpDriver {
    fn start(
        &mut self,
        profile_dir: &Path,
        start_url: &str,
        deadline: Instant,
    ) -> Result<(), BackendError> {
        ensure_before_deadline(deadline)?;
        if self.child.is_some() || self.cdp.is_some() || self.profile_lock_path.is_some() {
            return Err(BackendError::new(
                BackendErrorKind::Internal,
                "browser driver is already started",
            ));
        }
        if !(start_url.starts_with("https://") || start_url.starts_with("http://")) {
            return Err(BackendError::new(
                BackendErrorKind::NavigationFailed,
                "Brain start URL must use HTTP or HTTPS",
            ));
        }

        fs::create_dir_all(profile_dir)
            .map_err(|_| browser_unavailable("browser profile directory could not be created"))?;
        let lock_path = profile_dir.join(".oib-profile.lock");
        let mut lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|_| browser_unavailable("browser profile is already locked or not writable"))?;
        self.profile_lock_path = Some(lock_path);
        self._profile_lock = Some(lock);
        if let Some(lock) = self._profile_lock.as_mut() {
            writeln!(lock, "pid={}", std::process::id())
                .map_err(|_| browser_unavailable("browser profile lock could not be written"))?;
            lock.flush()
                .map_err(|_| browser_unavailable("browser profile lock could not be flushed"))?;
        }

        let port = free_loopback_port()?;
        self.child = Some(self.launch_browser(profile_dir, port)?);
        self.wait_for_debugger(port, deadline)?;
        let target = http_json(port, "PUT", "/json/new?about:blank", deadline)?;
        let target_websocket = string_field(&target, "webSocketDebuggerUrl")
            .ok_or_else(|| browser_unavailable("DevTools target did not return a WebSocket URL"))?;
        let mut cdp = CdpClient::connect(target_websocket, deadline)?;
        let _page_enabled = cdp.call("Page.enable", "{}", deadline)?;
        let _runtime_enabled = cdp.call("Runtime.enable", "{}", deadline)?;
        let params = format!(r#"{{"url":{}}}"#, json::quote_string(start_url));
        let _navigation = cdp.call("Page.navigate", &params, deadline)?;
        self.cdp = Some(cdp);
        self.baseline_count = 0;
        self.baseline_last_text.clear();
        self.last_candidate.clear();
        self.stable_samples = 0;
        Ok(())
    }

    fn readiness(&mut self, deadline: Instant) -> Result<Readiness, BackendError> {
        self.check_browser_alive()?;
        loop {
            ensure_before_deadline(deadline)?;
            let state = match self.page_state(deadline) {
                Ok(state) => state,
                Err(error) if error.kind == BackendErrorKind::Timeout => return Err(error),
                Err(_) => {
                    sleep_until(deadline, POLL_INTERVAL)?;
                    continue;
                }
            };
            let url = string_field(&state, "url").unwrap_or_default();
            let body = string_field(&state, "body").unwrap_or_default().to_ascii_lowercase();
            let editor = bool_field(&state, "editor").unwrap_or(false);

            if is_challenge_text(&body) {
                return Ok(Readiness::Challenge);
            }
            if is_rate_limit_text(&body) {
                return Ok(Readiness::RateLimited);
            }
            if url.contains("/auth/") || url.contains("/login") || (!editor && is_login_text(&body)) {
                return Ok(Readiness::LoginRequired);
            }
            if editor {
                return Ok(Readiness::Ready);
            }
            sleep_until(deadline, POLL_INTERVAL)?;
        }
    }

    fn submit_prompt(&mut self, prompt: &str, deadline: Instant) -> Result<(), BackendError> {
        ensure_before_deadline(deadline)?;
        let state = self.page_state(deadline)?;
        self.baseline_count = number_field(&state, "count").unwrap_or(0) as usize;
        self.baseline_last_text = string_field(&state, "last").unwrap_or_default().to_owned();
        self.last_candidate.clear();
        self.stable_samples = 0;

        let expression = format!(
            r#"JSON.stringify((()=>{{const p={};const e=document.querySelector('#prompt-textarea,[contenteditable="true"]');if(!e)return {{ok:false}};e.focus();if(e instanceof HTMLTextAreaElement||e instanceof HTMLInputElement){{const d=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(e),'value');if(d&&d.set)d.set.call(e,p);else e.value=p;e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText',data:p}}));}}else{{document.execCommand('selectAll');document.execCommand('insertText',false,p);e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText',data:p}}));}}const b=document.querySelector('button[data-testid="send-button"],button[data-testid="composer-send-button"],button[aria-label="Send prompt"]');if(!b||b.disabled)return {{ok:false}};b.click();return {{ok:true}};}})())"#,
            json::quote_string(prompt)
        );
        let result = self.evaluate_json(&expression, deadline)?;
        if bool_field(&result, "ok") == Some(true) {
            Ok(())
        } else {
            Err(BackendError::new(
                BackendErrorKind::SubmissionFailed,
                "provider prompt editor or send control was not available",
            ))
        }
    }

    fn next_snapshot(
        &mut self,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Option<TextSnapshot>, BackendError> {
        if cancellation.is_cancelled() {
            return Err(BackendError::new(
                BackendErrorKind::Cancelled,
                "inference was cancelled",
            ));
        }
        sleep_until(deadline, POLL_INTERVAL)?;
        let state = self.evaluate_json(
            r#"JSON.stringify((()=>{const n=Array.from(document.querySelectorAll('[data-message-author-role="assistant"]'));const last=n.length?n[n.length-1].innerText:'';const body=(document.body?.innerText||'').slice(0,4000).toLowerCase();const busy=!!document.querySelector('button[data-testid="stop-button"],button[aria-label*="Stop"]');return {count:n.length,last,busy,body};})())"#,
            deadline,
        )?;
        let body = string_field(&state, "body").unwrap_or_default();
        if is_challenge_text(body) {
            return Err(BackendError::new(
                BackendErrorKind::Challenge,
                "provider challenge detected during inference",
            ));
        }
        if is_rate_limit_text(body) {
            return Err(BackendError::new(
                BackendErrorKind::RateLimited,
                "provider rate limit detected during inference",
            ));
        }
        let count = number_field(&state, "count").unwrap_or(0) as usize;
        let text = string_field(&state, "last").unwrap_or_default();
        let is_new_response = count > self.baseline_count
            || (count == self.baseline_count && text != self.baseline_last_text);
        if !is_new_response || text.trim().is_empty() {
            return Ok(None);
        }

        let changed = text != self.last_candidate;
        if changed {
            self.last_candidate = text.to_owned();
            self.stable_samples = 0;
        } else {
            self.stable_samples = self.stable_samples.saturating_add(1);
        }
        let busy = bool_field(&state, "busy").unwrap_or(true);
        let complete = !busy && self.stable_samples >= 2;
        if changed || complete {
            Ok(Some(TextSnapshot {
                text: text.to_owned(),
                complete,
            }))
        } else {
            Ok(None)
        }
    }

    fn shutdown(&mut self, deadline: Instant) -> Result<(), BackendError> {
        self.cdp.take();
        if let Some(child) = self.child.as_ref() {
            #[cfg(windows)]
            {
                let pid = child.id().to_string();
                let _taskkill = Command::new("taskkill")
                    .args(["/PID", &pid, "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _kill = self.child.as_mut().map(Child::kill);
            }
            #[cfg(not(windows))]
            {
                let _kill = child.kill();
            }
        }

        loop {
            let exited = match self.child.as_mut() {
                Some(child) => child
                    .try_wait()
                    .map_err(|_| browser_unavailable("browser process status could not be checked"))?
                    .is_some(),
                None => true,
            };
            if exited {
                break;
            }
            if Instant::now() >= deadline {
                return Err(BackendError::new(
                    BackendErrorKind::Timeout,
                    "browser process did not exit before shutdown deadline",
                ));
            }
            thread::sleep(Duration::from_millis(25));
        }

        self.child.take();
        self._profile_lock.take();
        if let Some(path) = self.profile_lock_path.take() {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => {
                    return Err(browser_unavailable("browser profile lock could not be removed"));
                }
            }
        }
        Ok(())
    }
}

fn default_browser_candidates() -> Vec<&'static str> {
    #[cfg(target_os = "windows")]
    {
        vec!["msedge.exe", "chrome.exe", "msedge", "chrome"]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "chromium",
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            "microsoft-edge",
            "google-chrome",
            "chromium",
            "chromium-browser",
        ]
    }
}

fn free_loopback_port() -> Result<u16, BackendError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| browser_unavailable("could not reserve a local DevTools port"))?;
    let port = listener
        .local_addr()
        .map_err(|_| browser_unavailable("could not read the local DevTools port"))?
        .port();
    drop(listener);
    Ok(port)
}

fn http_json(
    port: u16,
    method: &str,
    path: &str,
    deadline: Instant,
) -> Result<JsonValue, BackendError> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let timeout = remaining(deadline)?;
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .map_err(|_| browser_unavailable("could not connect to local DevTools HTTP endpoint"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| browser_unavailable("could not configure DevTools socket timeout"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| browser_unavailable("could not configure DevTools socket timeout"))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|_| browser_unavailable("could not send DevTools HTTP request"))?;

    let head = read_http_head(&mut stream, deadline)?;
    let head_text = std::str::from_utf8(&head)
        .map_err(|_| browser_unavailable("DevTools HTTP response headers were malformed"))?;
    let status = head_text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| browser_unavailable("DevTools HTTP response status was malformed"))?;
    if !(200..300).contains(&status) {
        return Err(browser_unavailable("DevTools HTTP endpoint returned an error status"));
    }

    let content_length = head_text.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    });
    let mut body = Vec::new();
    if let Some(length) = content_length {
        if length > MAX_DEBUG_HTTP_BODY {
            return Err(browser_unavailable("DevTools HTTP response exceeded the size limit"));
        }
        body.resize(length, 0);
        stream
            .read_exact(&mut body)
            .map_err(|_| browser_unavailable("DevTools HTTP response body was truncated"))?;
    } else {
        let mut limited = (&mut stream).take((MAX_DEBUG_HTTP_BODY + 1) as u64);
        limited
            .read_to_end(&mut body)
            .map_err(|_| browser_unavailable("DevTools HTTP response body could not be read"))?;
        if body.len() > MAX_DEBUG_HTTP_BODY {
            return Err(browser_unavailable("DevTools HTTP response exceeded the size limit"));
        }
    }
    let body = std::str::from_utf8(&body)
        .map_err(|_| browser_unavailable("DevTools HTTP response was not UTF-8"))?;
    json::parse(body).map_err(|_| browser_unavailable("DevTools HTTP response was not valid JSON"))
}

fn read_http_head(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>, BackendError> {
    let mut head = Vec::with_capacity(1024);
    let mut byte = [0_u8; 1];
    loop {
        if head.len() >= MAX_HTTP_HEAD {
            return Err(browser_unavailable("DevTools HTTP headers exceeded the size limit"));
        }
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(|_| browser_unavailable("could not configure DevTools socket timeout"))?;
        match stream.read(&mut byte) {
            Ok(0) => return Err(browser_unavailable("DevTools closed the HTTP response early")),
            Ok(_) => head.push(byte[0]),
            Err(error)
                if error.kind() == io::ErrorKind::TimedOut
                    || error.kind() == io::ErrorKind::WouldBlock =>
            {
                return Err(BackendError::new(
                    BackendErrorKind::Timeout,
                    "DevTools HTTP operation timed out",
                ));
            }
            Err(_) => return Err(browser_unavailable("DevTools HTTP response could not be read")),
        }
        if head.ends_with(b"\r\n\r\n") {
            return Ok(head);
        }
    }
}

struct CdpClient {
    stream: TcpStream,
    next_id: u64,
}

impl CdpClient {
    fn connect(url: &str, deadline: Instant) -> Result<Self, BackendError> {
        let rest = url
            .strip_prefix("ws://")
            .ok_or_else(|| browser_unavailable("DevTools returned an unsupported WebSocket URL"))?;
        let (authority, path) = rest
            .split_once('/')
            .ok_or_else(|| browser_unavailable("DevTools WebSocket URL was malformed"))?;
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| browser_unavailable("DevTools WebSocket authority was malformed"))?;
        let port = port
            .parse::<u16>()
            .map_err(|_| browser_unavailable("DevTools WebSocket port was malformed"))?;
        let address = (host, port)
            .to_socket_addrs()
            .map_err(|_| browser_unavailable("DevTools WebSocket host could not be resolved"))?
            .next()
            .ok_or_else(|| browser_unavailable("DevTools WebSocket host could not be resolved"))?;
        let timeout = remaining(deadline)?;
        let mut stream = TcpStream::connect_timeout(&address, timeout)
            .map_err(|_| browser_unavailable("could not connect to DevTools WebSocket"))?;

        let nonce = websocket_nonce(port);
        let key = base64_encode(&nonce);
        let accept = base64_encode(&sha1(format!("{key}{WEBSOCKET_GUID}").as_bytes()));
        write!(
            stream,
            "GET /{path} HTTP/1.1\r\nHost: {authority}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        .map_err(|_| browser_unavailable("could not send DevTools WebSocket handshake"))?;
        let head = read_http_head(&mut stream, deadline)?;
        let head = std::str::from_utf8(&head)
            .map_err(|_| browser_unavailable("DevTools WebSocket handshake was malformed"))?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u16>().ok());
        let returned_accept = head.lines().skip(1).find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("sec-websocket-accept")
                .then(|| value.trim())
        });
        if status != Some(101) || returned_accept != Some(accept.as_str()) {
            return Err(browser_unavailable("DevTools rejected the WebSocket handshake"));
        }
        Ok(Self { stream, next_id: 1 })
    }

    fn call(
        &mut self,
        method: &str,
        params_json: &str,
        deadline: Instant,
    ) -> Result<JsonValue, BackendError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let message = format!(
            r#"{{"id":{id},"method":{},"params":{params_json}}}"#,
            json::quote_string(method)
        );
        self.send_text(message.as_bytes(), deadline)?;
        loop {
            let message = self.read_text(deadline)?;
            let value = json::parse(&message)
                .map_err(|_| browser_unavailable("DevTools returned malformed JSON"))?;
            let response_id = value
                .as_object()
                .and_then(|object| object.get("id"))
                .and_then(JsonValue::as_u64);
            if response_id != Some(id) {
                continue;
            }
            if value
                .as_object()
                .is_some_and(|object| object.contains_key("error"))
            {
                return Err(BackendError::new(
                    BackendErrorKind::Internal,
                    "DevTools command failed",
                ));
            }
            return Ok(value);
        }
    }

    fn evaluate_json(&mut self, expression: &str, deadline: Instant) -> Result<JsonValue, BackendError> {
        let params = format!(
            r#"{{"expression":{},"returnByValue":true,"awaitPromise":true}}"#,
            json::quote_string(expression)
        );
        let response = self.call("Runtime.evaluate", &params, deadline)?;
        let result = response
            .as_object()
            .and_then(|object| object.get("result"))
            .and_then(JsonValue::as_object)
            .and_then(|object| object.get("result"))
            .and_then(JsonValue::as_object)
            .ok_or_else(|| browser_unavailable("DevTools evaluation result was malformed"))?;
        if result.contains_key("exceptionDetails") {
            return Err(BackendError::new(
                BackendErrorKind::ExtractionFailed,
                "provider page evaluation failed",
            ));
        }
        let serialized = result
            .get("value")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| browser_unavailable("DevTools evaluation returned no value"))?;
        json::parse(serialized)
            .map_err(|_| browser_unavailable("provider page state was not valid JSON"))
    }

    fn send_text(&mut self, payload: &[u8], deadline: Instant) -> Result<(), BackendError> {
        self.write_frame(0x1, payload, deadline)
    }

    fn read_text(&mut self, deadline: Instant) -> Result<String, BackendError> {
        let mut fragments = Vec::new();
        loop {
            let (final_frame, opcode, payload) = self.read_frame(deadline)?;
            match opcode {
                0x1 if final_frame => {
                    if !fragments.is_empty() {
                        return Err(browser_unavailable("DevTools WebSocket fragmentation was malformed"));
                    }
                    return String::from_utf8(payload)
                        .map_err(|_| browser_unavailable("DevTools WebSocket message was not UTF-8"));
                }
                0x1 => {
                    if !fragments.is_empty() {
                        return Err(browser_unavailable("DevTools WebSocket fragmentation was malformed"));
                    }
                    fragments = payload;
                }
                0x0 => {
                    if fragments.is_empty() {
                        return Err(browser_unavailable("unexpected WebSocket continuation frame"));
                    }
                    fragments.extend(payload);
                    if final_frame {
                        return String::from_utf8(fragments)
                            .map_err(|_| browser_unavailable("DevTools WebSocket message was not UTF-8"));
                    }
                }
                0x8 => return Err(browser_unavailable("DevTools WebSocket closed")),
                0x9 => self.write_frame(0xA, &payload, deadline)?,
                0xA => {}
                _ => return Err(browser_unavailable("unsupported DevTools WebSocket frame")),
            }
        }
    }

    fn write_frame(
        &mut self,
        opcode: u8,
        payload: &[u8],
        deadline: Instant,
    ) -> Result<(), BackendError> {
        let mut header = Vec::with_capacity(14);
        header.push(0x80 | opcode);
        if payload.len() <= 125 {
            header.push(0x80 | payload.len() as u8);
        } else if payload.len() <= u16::MAX as usize {
            header.push(0x80 | 126);
            header.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            header.push(0x80 | 127);
            header.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u32;
        let mask = (time ^ MASK_COUNTER.fetch_add(1, Ordering::Relaxed)).to_be_bytes();
        header.extend_from_slice(&mask);
        let masked = payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4])
            .collect::<Vec<_>>();
        self.stream
            .set_write_timeout(Some(remaining(deadline)?))
            .map_err(|_| browser_unavailable("could not configure WebSocket write timeout"))?;
        self.stream
            .write_all(&header)
            .and_then(|_| self.stream.write_all(&masked))
            .map_err(|_| BackendError::new(BackendErrorKind::Timeout, "DevTools write timed out"))
    }

    fn read_frame(&mut self, deadline: Instant) -> Result<(bool, u8, Vec<u8>), BackendError> {
        let mut first = [0_u8; 2];
        self.read_exact_deadline(&mut first, deadline)?;
        let final_frame = first[0] & 0x80 != 0;
        let opcode = first[0] & 0x0f;
        let masked = first[1] & 0x80 != 0;
        let mut length = (first[1] & 0x7f) as u64;
        if length == 126 {
            let mut extended = [0_u8; 2];
            self.read_exact_deadline(&mut extended, deadline)?;
            length = u16::from_be_bytes(extended) as u64;
        } else if length == 127 {
            let mut extended = [0_u8; 8];
            self.read_exact_deadline(&mut extended, deadline)?;
            length = u64::from_be_bytes(extended);
        }
        if length > MAX_WEBSOCKET_FRAME as u64 {
            return Err(browser_unavailable("DevTools WebSocket frame exceeded the size limit"));
        }
        let mask = if masked {
            let mut mask = [0_u8; 4];
            self.read_exact_deadline(&mut mask, deadline)?;
            Some(mask)
        } else {
            None
        };
        let mut payload = vec![0_u8; length as usize];
        self.read_exact_deadline(&mut payload, deadline)?;
        if let Some(mask) = mask {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }
        Ok((final_frame, opcode, payload))
    }

    fn read_exact_deadline(&mut self, buffer: &mut [u8], deadline: Instant) -> Result<(), BackendError> {
        self.stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(|_| browser_unavailable("could not configure WebSocket read timeout"))?;
        self.stream.read_exact(buffer).map_err(|error| {
            if error.kind() == io::ErrorKind::TimedOut
                || error.kind() == io::ErrorKind::WouldBlock
            {
                BackendError::new(BackendErrorKind::Timeout, "DevTools read timed out")
            } else {
                browser_unavailable("DevTools WebSocket read failed")
            }
        })
    }
}

fn string_field<'a>(value: &'a JsonValue, key: &str) -> Option<&'a str> {
    value.as_object()?.get(key)?.as_str()
}

fn bool_field(value: &JsonValue, key: &str) -> Option<bool> {
    value.as_object()?.get(key)?.as_bool()
}

fn number_field(value: &JsonValue, key: &str) -> Option<u64> {
    value.as_object()?.get(key)?.as_u64()
}

fn is_challenge_text(body: &str) -> bool {
    [
        "verify you are human",
        "security check",
        "captcha",
        "unusual activity",
        "confirm you are not a robot",
    ]
    .iter()
    .any(|needle| body.contains(needle))
}

fn is_rate_limit_text(body: &str) -> bool {
    [
        "too many requests",
        "rate limit",
        "try again later",
        "usage limit",
        "you've reached the current limit",
    ]
    .iter()
    .any(|needle| body.contains(needle))
}

fn is_login_text(body: &str) -> bool {
    body.contains("log in") || body.contains("sign up") || body.contains("sign in")
}

fn ensure_before_deadline(deadline: Instant) -> Result<(), BackendError> {
    remaining(deadline).map(|_| ())
}

fn remaining(deadline: Instant) -> Result<Duration, BackendError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(BackendError::new(
            BackendErrorKind::Timeout,
            "browser operation deadline expired",
        ))
    } else {
        Ok(remaining)
    }
}

fn sleep_until(deadline: Instant, interval: Duration) -> Result<(), BackendError> {
    let remaining = remaining(deadline)?;
    thread::sleep(interval.min(remaining));
    ensure_before_deadline(deadline)
}

fn browser_unavailable(message: &'static str) -> BackendError {
    BackendError::new(BackendErrorKind::BrowserUnavailable, message)
}

fn websocket_nonce(port: u16) -> [u8; 16] {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let counter = MASK_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut nonce = [0_u8; 16];
    nonce[..8].copy_from_slice(&nanos.to_be_bytes());
    nonce[8..12].copy_from_slice(&std::process::id().to_be_bytes());
    nonce[12..14].copy_from_slice(&port.to_be_bytes());
    nonce[14..].copy_from_slice(&(counter as u16).to_be_bytes());
    nonce
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(c & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn sha1(input: &[u8]) -> [u8; 20] {
    let mut data = input.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = [
        0x67452301_u32,
        0xefcdab89,
        0x98badcfe,
        0x10325476,
        0xc3d2e1f0,
    ];
    for block in data.chunks_exact(64) {
        let mut words = [0_u32; 80];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                block[offset],
                block[offset + 1],
                block[offset + 2],
                block[offset + 3],
            ]);
        }
        for index in 16..80 {
            words[index] = (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                .rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (index, word) in words.iter().enumerate() {
            let (function, constant) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a827999),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(function)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut digest = [0_u8; 20];
    for (index, word) in h.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn sha1_and_base64_match_known_vectors() {
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
    }

    #[test]
    fn readiness_text_detection_is_conservative() {
        assert!(is_challenge_text("Please verify you are human"));
        assert!(is_rate_limit_text("Too many requests, try again later"));
        assert!(is_login_text("Please log in to continue"));
        assert!(!is_challenge_text("Hello, how can I help?"));
    }

    #[test]
    fn websocket_nonce_changes_between_calls() {
        assert_ne!(websocket_nonce(9222), websocket_nonce(9222));
    }
}
