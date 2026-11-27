use std::ffi::c_void;
use std::io;
use std::ptr::{self, NonNull};

use heka_core::web::{Answer, Request, Web};
use windows_sys::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
    WINHTTP_DISABLE_AUTHENTICATION, WINHTTP_DISABLE_COOKIES, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts,
};

const TIMEOUT_MS: i32 = 10_000;
const BODY_LIMIT: usize = 1024 * 1024;
const CHUNK: usize = 16 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct WinHttp;

impl Web for WinHttp {
    fn get(&self, request: &Request) -> Result<Answer, String> {
        fetch(request, &Route::HTTPS)
    }
}

struct Route {
    access: WINHTTP_ACCESS_TYPE,
    port: u16,
    flags: WINHTTP_OPEN_REQUEST_FLAGS,
}

impl Route {
    const HTTPS: Self = Self {
        access: WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
        port: INTERNET_DEFAULT_HTTPS_PORT,
        flags: WINHTTP_FLAG_SECURE,
    };

    #[cfg(test)]
    fn local(port: u16) -> Self {
        Self {
            access: windows_sys::Win32::Networking::WinHttp::WINHTTP_ACCESS_TYPE_NO_PROXY,
            port,
            flags: 0,
        }
    }
}

struct Handle(NonNull<c_void>);

impl Handle {
    fn opened(handle: *mut c_void) -> Result<Self, String> {
        NonNull::new(handle).map(Self).ok_or_else(last_error)
    }

    fn get(&self) -> *mut c_void {
        self.0.as_ptr()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { WinHttpCloseHandle(self.get()) };
    }
}

fn fetch(request: &Request, route: &Route) -> Result<Answer, String> {
    let agent = wide(&request.user_agent);
    let session = Handle::opened(unsafe {
        WinHttpOpen(agent.as_ptr(), route.access, ptr::null(), ptr::null(), 0)
    })?;
    done(unsafe {
        WinHttpSetTimeouts(
            session.get(),
            TIMEOUT_MS,
            TIMEOUT_MS,
            TIMEOUT_MS,
            TIMEOUT_MS,
        )
    })?;
    let host = wide(request.host);
    let connection =
        Handle::opened(unsafe { WinHttpConnect(session.get(), host.as_ptr(), route.port, 0) })?;
    let verb = wide("GET");
    let path = wide(&request.path);
    let sent = Handle::opened(unsafe {
        WinHttpOpenRequest(
            connection.get(),
            verb.as_ptr(),
            path.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            route.flags,
        )
    })?;
    send(&sent, request.accept)?;
    Ok(Answer {
        status: status(&sent)?,
        body: body(&sent)?,
    })
}

fn send(request: &Handle, accept: &str) -> Result<(), String> {
    let disabled: u32 = WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_AUTHENTICATION;
    done(unsafe {
        WinHttpSetOption(
            request.get(),
            WINHTTP_OPTION_DISABLE_FEATURE,
            ptr::from_ref(&disabled).cast(),
            size_of::<u32>() as u32,
        )
    })?;
    let headers: Vec<u16> = format!("Accept: {accept}").encode_utf16().collect();
    done(unsafe {
        WinHttpSendRequest(
            request.get(),
            headers.as_ptr(),
            headers.len() as u32,
            ptr::null(),
            0,
            0,
            0,
        )
    })?;
    done(unsafe { WinHttpReceiveResponse(request.get(), ptr::null_mut()) })
}

fn status(request: &Handle) -> Result<u16, String> {
    let mut status: u32 = 0;
    let mut len = size_of::<u32>() as u32;
    done(unsafe {
        WinHttpQueryHeaders(
            request.get(),
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            ptr::from_mut(&mut status).cast(),
            &mut len,
            ptr::null_mut(),
        )
    })?;
    u16::try_from(status).map_err(|_| format!("the status {status} is not one"))
}

fn body(request: &Handle) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    let mut chunk = [0u8; CHUNK];
    loop {
        let mut read = 0;
        done(unsafe {
            WinHttpReadData(
                request.get(),
                chunk.as_mut_ptr().cast(),
                CHUNK as u32,
                &mut read,
            )
        })?;
        if read == 0 {
            return Ok(body);
        }
        body.extend_from_slice(&chunk[..read as usize]);
        if body.len() > BODY_LIMIT {
            return Err("the answer is too long".into());
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn done(ok: i32) -> Result<(), String> {
    if ok == 0 {
        return Err(last_error());
    }
    Ok(())
}

fn last_error() -> String {
    let code = io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_default();
    format!("WinHTTP error {code}")
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    fn served(answer: Vec<u8>) -> (Result<Answer, String>, Vec<String>) {
        let server = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let port = server.local_addr().expect("its number").port();
        let received = thread::spawn(move || {
            let (mut stream, _) = server.accept().expect("a connection");
            let lines: Vec<String> = BufReader::new(&stream)
                .lines()
                .map(|line| line.expect("a line of the request"))
                .take_while(|line| !line.is_empty())
                .collect();
            let _ = stream.write_all(&answer);
            lines
        });
        let answer = fetch(&request(), &Route::local(port));
        let mut received = received.join().expect("the server ran");
        for line in &mut received {
            *line = line.replace(&port.to_string(), "PORT");
        }
        (answer, received)
    }

    fn request() -> Request {
        Request {
            host: "127.0.0.1",
            path: "/repos/someone/heka/releases/latest".into(),
            user_agent: "heka/1.2.3".into(),
            accept: "application/vnd.github+json",
        }
    }

    fn answer(status: &str, body: &[u8]) -> Vec<u8> {
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        [head.as_bytes(), body].concat()
    }

    #[test]
    fn the_request_carries_nothing_but_what_it_is_given() {
        let (_, mut received) = served(answer("200 OK", b"{}"));

        assert_eq!(
            received.remove(0),
            "GET /repos/someone/heka/releases/latest HTTP/1.1"
        );
        received.sort();
        assert_eq!(
            received,
            [
                "Accept: application/vnd.github+json",
                "Connection: Keep-Alive",
                "Host: 127.0.0.1:PORT",
                "User-Agent: heka/1.2.3",
            ]
        );
    }

    #[test]
    fn the_status_and_the_whole_body_come_back() {
        let body = vec![b'x'; 100_000];

        let (answered, _) = served(answer("404 Not Found", &body));

        assert_eq!(answered, Ok(Answer { status: 404, body }));
    }

    #[test]
    fn an_answer_too_long_to_be_a_release_is_refused() {
        let (answered, _) = served(answer("200 OK", &vec![b'x'; BODY_LIMIT + 1]));

        assert_eq!(answered, Err("the answer is too long".into()));
    }

    #[test]
    fn no_one_listening_is_an_error() {
        let free = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let port = free.local_addr().expect("its number").port();
        drop(free);

        let answered = fetch(&request(), &Route::local(port));

        assert!(
            answered
                .as_ref()
                .is_err_and(|error| error.starts_with("WinHTTP error ")),
            "{answered:?}"
        );
    }
}
