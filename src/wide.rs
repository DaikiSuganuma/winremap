//! UTF-8 to UTF-16 for the Win32 calls that take wide strings — one place
//! for the conversion (project brief §5-7). The `windows` crate's `HSTRING`
//! and `w!` cover the calls that take a `PCWSTR`; these two are for the GDI
//! buffers that take raw `u16`s. No `unsafe` here.

/// `text` as UTF-16 code units, without a terminator — for `DrawTextW` and
/// `GetTextExtentPoint32W`, which take a length.
pub fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Writes `face` into a fixed buffer such as `LOGFONTW::lfFaceName`, which
/// is zeroed beforehand: what does not fit is dropped, and the zero that
/// follows the last written unit is the terminator GDI reads.
pub fn copy_face_name(buffer: &mut [u16], face: &str) {
    for (slot, unit) in buffer.iter_mut().zip(face.encode_utf16()) {
        *slot = unit;
    }
}
