//! Minimal HTTP client on top of WinHTTP.
//!
//! The relay health checks, the public Funnel verification and the Node.js
//! download all go through here. Using the OS stack keeps the build free of a
//! TLS dependency and gives the same certificate behaviour as a browser.

use std::ffi::c_void;
use std::io::Write;
use std::path::Path;

use windows_sys::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE,
};

fn wide(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

pub struct Response {
    pub status: u16,
    pub body: String,
}

struct Target {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn split_url(url: &str) -> Result<Target, String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("invalid URL: {url}"))?;
    let secure = scheme.eq_ignore_ascii_case("https");
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err(format!("invalid URL: {url}"));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => (
            host.to_string(),
            port.parse::<u16>()
                .unwrap_or(if secure { 443 } else { 80 }),
        ),
        _ => (
            authority.to_string(),
            if secure { 443 } else { 80 },
        ),
    };
    Ok(Target {
        secure,
        host,
        port,
        path: path.to_string(),
    })
}

fn open_session() -> Result<Handle, String> {
    let agent = wide("DhakaFlixRelay/1.0");
    let handle = unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    };
    if handle.is_null() {
        return Err("WinHTTP session could not be created".to_string());
    }
    Ok(Handle(handle))
}

fn open_request(
    session: &Handle,
    target: &Target,
    timeout_ms: u32,
) -> Result<Handle, String> {
    let timeout = timeout_ms.max(500).min(60_000) as i32;
    unsafe {
        WinHttpSetTimeouts(session.0, timeout, timeout, timeout, timeout);
    }

    let host = wide(&target.host);
    let connect = unsafe { WinHttpConnect(session.0, host.as_ptr(), target.port, 0) };
    if connect.is_null() {
        return Err(format!("cannot connect to {}:{}", target.host, target.port));
    }
    let connect = Handle(connect);

    let verb = wide("GET");
    let path = wide(&target.path);
    let secure = if target.secure { WINHTTP_FLAG_SECURE } else { 0 };
    let request = unsafe {
        WinHttpOpenRequest(
            connect.0,
            verb.as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            secure,
        )
    };
    if request.is_null() {
        return Err(format!("request could not be created for {}", target.host));
    }
    // Keep the connection handle alive for the lifetime of the request.
    std::mem::forget(connect);
    Ok(Handle(request))
}

/// Fetch a URL. Errors describe the transport problem, not the HTTP status.
pub fn get(url: &str, timeout_ms: u32) -> Result<Response, String> {
    let target = split_url(url)?;
    let session = open_session()?;
    let request = open_request(&session, &target, timeout_ms)?;

    let sent = unsafe {
        WinHttpSendRequest(
            request.0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
            0,
            0,
        )
    };
    if sent == 0 {
        return Err(format!("request to {} failed", target.host));
    }
    if unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) } == 0 {
        return Err(format!("{} did not answer", target.host));
    }

    let mut status: u32 = 0;
    let mut status_size: u32 = std::mem::size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut status as *mut u32 as *mut c_void,
            &mut status_size,
            std::ptr::null_mut(),
        );
    }

    let body = read_body(request.0, None)?;
    Ok(Response {
        status: status as u16,
        body,
    })
}

fn read_body(
    request: *mut c_void,
    mut sink: Option<&mut std::fs::File>,
) -> Result<String, String> {
    let mut buffer = vec![0u8; 32 * 1024];
    let mut bytes = Vec::new();

    loop {
        let mut read: u32 = 0;
        let ok = unsafe {
            WinHttpReadData(
                request,
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as u32,
                &mut read,
            )
        };
        if ok == 0 || read == 0 {
            break;
        }
        let chunk = &buffer[..read as usize];
        match sink.as_mut() {
            Some(file) => {
                if file.write_all(chunk).is_err() {
                    return Err("could not write the download to disk".to_string());
                }
            }
            None => bytes.extend_from_slice(chunk),
        }
    }

    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Download a URL straight to a file, streaming so large archives stay out of
/// memory.
pub fn download(url: &str, destination: &Path, timeout_ms: u32) -> Result<u64, String> {
    let target = split_url(url)?;
    let session = open_session()?;
    let request = open_request(&session, &target, timeout_ms)?;

    let sent = unsafe {
        WinHttpSendRequest(request.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0)
    };
    if sent == 0 {
        return Err("download request failed".to_string());
    }
    if unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) } == 0 {
        return Err("download did not start".to_string());
    }

    let mut status: u32 = 0;
    let mut status_size: u32 = std::mem::size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut status as *mut u32 as *mut c_void,
            &mut status_size,
            std::ptr::null_mut(),
        );
    }
    if status != 200 {
        return Err(format!("server answered HTTP {status}"));
    }

    let mut file = std::fs::File::create(destination).map_err(|error| error.to_string())?;
    read_body(request.0, Some(&mut file))?;
    file.flush().map_err(|error| error.to_string())?;
    Ok(file.metadata().map(|meta| meta.len()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::split_url;

    #[test]
    fn parses_urls() {
        let target = split_url("https://dhakaflix.example.ts.net/manifest.json").unwrap();
        assert!(target.secure);
        assert_eq!(target.host, "dhakaflix.example.ts.net");
        assert_eq!(target.port, 443);
        assert_eq!(target.path, "/manifest.json");

        let local = split_url("http://127.0.0.1:7001/health").unwrap();
        assert!(!local.secure);
        assert_eq!(local.host, "127.0.0.1");
        assert_eq!(local.port, 7001);
        assert_eq!(local.path, "/health");
    }
}
