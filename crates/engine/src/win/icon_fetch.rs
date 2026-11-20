//! A system page's icon, fetched outside every profile's data: `https://`
//! only, without cookies and without a name or password in the address, a
//! redirect followed only to such an address, and no more than an icon may
//! be. Kept as a PNG (`favicon::store_icon`), decoded by Windows' own
//! imaging component (ICO, PNG, JPEG, GIF, BMP).

use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
    IWICBitmapFrameDecode, IWICImagingFactory, WICBitmapEncoderNoCache, WICConvertBitmapSource,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_DISABLE_COOKIES, WINHTTP_DISABLE_REDIRECTS,
    WINHTTP_FLAG_SECURE, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_LOCATION, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetOption,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
};
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows_core::{HSTRING, PCWSTR, w};

/// The largest icon a site may hand over: more is not an icon.
const ICON_LIMIT: usize = 1 << 20;
/// How many redirects an icon's address may take.
const REDIRECTS: usize = 5;

/// Fetch the image at `url` and keep it as a PNG at `path`, on a thread of
/// its own.
pub(crate) fn fetch_icon(url: &str, path: PathBuf) {
    if parts(url).is_none() {
        return;
    }
    let url = url.to_string();
    let _ = std::thread::Builder::new()
        .name("system page icon".into())
        .spawn(move || {
            if let Some(png) = fetch(&url).and_then(|bytes| png_of(&bytes)) {
                crate::favicon::store_icon(&path, &png);
            }
        });
}

/// An `https://` address as host, port and the path with its query; none
/// for any other address, and for one with a name or password in it.
fn parts(url: &str) -> Option<(String, u16, String)> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let rest = rest.split('#').next()?;
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        // Not the colons of an IPv6 address.
        Some((host, port)) if !port.contains(']') => (host, port.parse().ok()?),
        _ => (authority, 443),
    };
    let path = if path.is_empty() {
        "/".to_string()
    } else if path.starts_with('?') {
        format!("/{path}")
    } else {
        path.to_string()
    };
    Some((host.to_string(), port, path))
}

/// Where a redirect from `from` to `location` leads: an address of its own,
/// or a path on the same host.
fn redirected(from: &str, location: &str) -> Option<String> {
    if location.contains("://") {
        return Some(location.to_string());
    }
    let (host, port, _) = parts(from)?;
    location
        .starts_with('/')
        .then(|| format!("https://{host}:{port}{location}"))
}

/// The body at `url`, following redirects that `parts` allows, if it is no
/// more than `ICON_LIMIT`.
fn fetch(url: &str) -> Option<Vec<u8>> {
    // SAFETY: WinHTTP handles opened and closed here.
    unsafe {
        let session = WinHttpOpen(
            w!("Magnetite"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            return None;
        }
        let mut url = url.to_string();
        let mut body = None;
        for _ in 0..=REDIRECTS {
            match get(session, &url) {
                Some(Answer::Body(bytes)) => {
                    body = Some(bytes);
                    break;
                }
                Some(Answer::Redirect(location)) => {
                    match redirected(&url, &location).filter(|u| parts(u).is_some()) {
                        Some(next) => url = next,
                        None => break,
                    }
                }
                None => break,
            }
        }
        let _ = WinHttpCloseHandle(session);
        body
    }
}

enum Answer {
    Body(Vec<u8>),
    Redirect(String),
}

/// One request, without cookies and without following a redirect.
///
/// # Safety
/// `session` is an open WinHTTP session.
unsafe fn get(session: *mut c_void, url: &str) -> Option<Answer> {
    let (host, port, path) = parts(url)?;
    // SAFETY: handles opened here and closed before returning.
    unsafe {
        let connection = WinHttpConnect(session, &HSTRING::from(host), port, 0);
        if connection.is_null() {
            return None;
        }
        let request = WinHttpOpenRequest(
            connection,
            w!("GET"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        );
        let answer = (!request.is_null()).then(|| answer(request)).flatten();
        if !request.is_null() {
            let _ = WinHttpCloseHandle(request);
        }
        let _ = WinHttpCloseHandle(connection);
        answer
    }
}

/// Send `request` and read what comes back.
///
/// # Safety
/// `request` is an open WinHTTP request.
unsafe fn answer(request: *mut c_void) -> Option<Answer> {
    // SAFETY: an open request; buffers of the sizes given.
    unsafe {
        let off = WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_REDIRECTS;
        WinHttpSetOption(
            Some(request),
            WINHTTP_OPTION_DISABLE_FEATURE,
            Some(&off.to_ne_bytes()),
        )
        .ok()?;
        WinHttpSendRequest(request, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request, std::ptr::null_mut()).ok()?;
        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &raw mut size,
            std::ptr::null_mut(),
        )
        .ok()?;
        if (300..400).contains(&status) {
            let mut buffer = [0u16; 2048];
            let mut size = u32::try_from(std::mem::size_of_val(&buffer)).ok()?;
            WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_LOCATION,
                PCWSTR::null(),
                Some(buffer.as_mut_ptr().cast()),
                &raw mut size,
                std::ptr::null_mut(),
            )
            .ok()?;
            let len = usize::try_from(size).ok()? / 2;
            return Some(Answer::Redirect(String::from_utf16_lossy(&buffer[..len])));
        }
        if status != 200 {
            return None;
        }
        let mut body = Vec::new();
        let mut chunk = vec![0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(
                request,
                chunk.as_mut_ptr().cast(),
                u32::try_from(chunk.len()).ok()?,
                &raw mut read,
            )
            .ok()?;
            if read == 0 {
                break;
            }
            let read = usize::try_from(read).ok()?;
            // Ended as soon as more than an icon arrived.
            if body.len() + read > ICON_LIMIT {
                return None;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        Some(Answer::Body(body))
    }
}

/// The image in `bytes` as a PNG: of a file with several sizes, the largest.
fn png_of(bytes: &[u8]) -> Option<Vec<u8>> {
    // SAFETY: COM on this thread of ours, for the calls below only.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let png = encode(bytes);
    if com {
        // SAFETY: balances the call above.
        unsafe { CoUninitialize() };
    }
    png
}

fn encode(bytes: &[u8]) -> Option<Vec<u8>> {
    // SAFETY: WIC objects made and used on this thread; the streams copy or
    // own their memory.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let input = SHCreateMemStream(Some(bytes))?;
        let decoder = factory
            .CreateDecoderFromStream(&input, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .ok()?;
        let mut largest: Option<(u64, IWICBitmapFrameDecode)> = None;
        for index in 0..decoder.GetFrameCount().ok()? {
            let frame = decoder.GetFrame(index).ok()?;
            let (mut width, mut height) = (0u32, 0u32);
            frame.GetSize(&raw mut width, &raw mut height).ok()?;
            let area = u64::from(width) * u64::from(height);
            if largest.as_ref().is_none_or(|(a, _)| area > *a) {
                largest = Some((area, frame));
            }
        }
        let (_, frame) = largest?;
        let pixels = WICConvertBitmapSource(&GUID_WICPixelFormat32bppBGRA, &frame).ok()?;
        let output = SHCreateMemStream(None)?;
        let encoder = factory
            .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
            .ok()?;
        encoder.Initialize(&output, WICBitmapEncoderNoCache).ok()?;
        let mut target = None;
        encoder
            .CreateNewFrame(&raw mut target, std::ptr::null_mut())
            .ok()?;
        let target = target?;
        target.Initialize(None).ok()?;
        let (mut width, mut height) = (0u32, 0u32);
        pixels.GetSize(&raw mut width, &raw mut height).ok()?;
        target.SetSize(width, height).ok()?;
        let mut format = GUID_WICPixelFormat32bppBGRA;
        target.SetPixelFormat(&raw mut format).ok()?;
        target.WriteSource(&pixels, std::ptr::null()).ok()?;
        target.Commit().ok()?;
        encoder.Commit().ok()?;
        let mut stat = STATSTG::default();
        output.Stat(&raw mut stat, STATFLAG_NONAME).ok()?;
        let len = usize::try_from(stat.cbSize).ok()?;
        output.Seek(0, STREAM_SEEK_SET, None).ok()?;
        let mut png = vec![0u8; len];
        let mut read = 0u32;
        output
            .Read(
                png.as_mut_ptr().cast(),
                u32::try_from(len).ok()?,
                Some(&raw mut read),
            )
            .ok()
            .ok()?;
        png.truncate(usize::try_from(read).ok()?);
        Some(png)
    }
}

#[cfg(test)]
mod tests {
    use super::{parts, png_of, redirected};

    #[test]
    fn only_https_addresses_without_credentials() {
        assert_eq!(
            parts("https://www.netflix.com/favicon.ico"),
            Some(("www.netflix.com".into(), 443, "/favicon.ico".into()))
        );
        assert_eq!(
            parts("https://a.example:8443"),
            Some(("a.example".into(), 8443, "/".into()))
        );
        assert_eq!(
            parts("https://a.example?x=1#y"),
            Some(("a.example".into(), 443, "/?x=1".into()))
        );
        assert_eq!(parts("http://a.example/icon.png"), None);
        assert_eq!(parts("https://user:secret@a.example/icon.png"), None);
        assert_eq!(parts("data:image/png;base64,AAAA"), None);
        assert_eq!(parts("https:///icon.png"), None);
    }

    #[test]
    fn a_redirect_stays_on_https() {
        assert_eq!(
            redirected("https://a.example/x", "/icons/a.png").as_deref(),
            Some("https://a.example:443/icons/a.png")
        );
        let away = redirected("https://a.example/x", "http://b.example/a.png").unwrap();
        assert!(parts(&away).is_none());
        assert_eq!(redirected("https://a.example/x", "a.png"), None);
    }

    #[test]
    fn an_icon_becomes_a_png() {
        // A 1×1 GIF.
        let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\x00\x00\x00\x00\x00!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
        let png = png_of(gif).expect("decoded");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png_of(b"not an image").is_none());
    }
}
