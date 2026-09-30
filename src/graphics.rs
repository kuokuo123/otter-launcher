// Estimate the bottom row reached by the overlay's *rendered* content.
// Graphics occupy cells even when their escape sequences contain no newlines.

use std::sync::Mutex;
use terminal_size::{Height, terminal_size};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Geometry {
    rows: usize,
    cell_px: Option<usize>,
}

impl Geometry {
    fn current() -> Self {
        let rows = terminal_size().map(|(_, Height(n))| usize::from(n));
        // Unlike a terminal query, TIOCGWINSZ cannot swallow a pending key
        // press or block the editor waiting for a reply.
        #[cfg(unix)]
        let pixels = {
            use std::os::fd::AsRawFd;
            let mut size = libc::winsize {
                ws_row: 0,
                ws_col: 0,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            let ok = unsafe {
                libc::ioctl(std::io::stdout().as_raw_fd(), libc::TIOCGWINSZ, &mut size)
            } == 0;
            (ok && size.ws_row > 0 && size.ws_ypixel > 0)
                .then_some((usize::from(size.ws_row), usize::from(size.ws_ypixel)))
        };
        #[cfg(not(unix))]
        let pixels: Option<(usize, usize)> = None;

        let rows = pixels.map(|p| p.0).or(rows).unwrap_or(24).max(1);
        // Round down so the pixel-to-cell conversion cannot underreserve.
        let cell_px = pixels.map(|p| (p.1 / rows).max(1));
        Self { rows, cell_px }
    }

    fn sixel_rows(self, pixels: usize) -> usize {
        self.cell_px.map_or(self.rows, |h| pixels.div_ceil(h))
    }
}

// overlay_cmd is executed once. Resize or font changes invalidate its extent.
static EXTENT: Mutex<Option<(Geometry, usize)>> = Mutex::new(None);

pub fn overlay_rows(output: &str) -> usize {
    let geometry = Geometry::current();
    let mut cache = EXTENT.lock().unwrap();
    if let Some((old, rows)) = *cache {
        if old == geometry {
            return rows;
        }
    }
    let rows = measure_overlay(output, geometry);
    *cache = Some((geometry, rows));
    rows
}

fn string_end(bytes: &[u8], mut at: usize, allow_bel: bool) -> Option<(usize, usize)> {
    while at < bytes.len() {
        if bytes[at] == 0x1b && bytes.get(at + 1) == Some(&b'\\') {
            return Some((at, at + 2));
        }
        if bytes[at] == 0xc2 && bytes.get(at + 1) == Some(&0x9c) {
            return Some((at, at + 2)); // UTF-8 encoding of ST
        }
        if allow_bel && bytes[at] == 7 {
            return Some((at, at + 1));
        }
        at += 1;
    }
    None
}

fn tmux_payload(bytes: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut decoded = Vec::new();
    loop {
        match (bytes.get(at), bytes.get(at + 1)) {
            (Some(&0x1b), Some(&0x1b)) => {
                decoded.push(0x1b); // tmux doubles embedded ESC bytes
                at += 2;
            }
            (Some(&0x1b), Some(&b'\\')) => {
                return String::from_utf8(decoded).ok().map(|s| (s, at + 2));
            }
            (Some(&byte), _) => {
                decoded.push(byte);
                at += 1;
            }
            _ => return None,
        }
    }
}

#[derive(Clone, Copy)]
struct Placement {
    rows: Option<usize>,
    move_cursor: bool,
}

fn kitty_command(body: &str) -> (Option<Placement>, bool) {
    let params = body.split_once(';').map_or(body, |(p, _)| p);
    let mut action = "t";
    let mut rows = None;
    let mut move_cursor = true;
    let mut virtual_image = false;
    let mut relative = false;
    let mut more = false;
    for param in params.split(',') {
        if let Some((key, value)) = param.split_once('=') {
            match key {
                "a" => action = value,
                "r" => rows = value.parse::<usize>().ok().filter(|&n| n > 0),
                "C" => move_cursor = value != "1",
                "U" => virtual_image = value == "1",
                "P" => relative = true,
                "m" => more = value == "1",
                _ => {}
            }
        }
    }
    let placement = if (action == "T" || action == "p") && !virtual_image {
        Some(Placement {
            // A relative placement can be offset from a different image, so
            // its own r value is not a bound on its bottom edge.
            rows: if relative { None } else { rows },
            move_cursor: move_cursor && !relative,
        })
    } else {
        None
    };
    (placement, more)
}

fn sixel_pixels(body: &str) -> Option<usize> {
    let (header, data) = body.split_once('q')?;
    if !header.bytes().all(|b| b.is_ascii_digit() || b == b';') {
        return None; // not a sixel DCS
    }
    // DECGRA: "Pan;Pad;Ph;Pv. Pv is the declared height in pixels.
    let declared = data.find('"').and_then(|at| {
        let attrs = &data[at + 1..];
        let end = attrs
            .bytes()
            .position(|b| !b.is_ascii_digit() && b != b';')
            .unwrap_or(attrs.len());
        attrs[..end].split(';').nth(3)?.parse::<usize>().ok()
    });
    // The data may omit DECGRA. Each '-' advances to another six-pixel band;
    // '$' returns to the beginning of the current band.
    let bands = if data.bytes().any(|b| (b'?'..=b'~').contains(&b)) {
        1usize.saturating_add(data.bytes().filter(|&b| b == b'-').count())
    } else {
        0
    };
    let pixels = declared.unwrap_or(0).max(bands.saturating_mul(6));
    (pixels > 0).then_some(pixels)
}

fn measure_overlay(output: &str, geometry: Geometry) -> usize {
    if output.is_empty() {
        return 0;
    }
    let bytes = output.as_bytes();
    let mut at = 0;
    let mut row = 0usize;
    let mut bottom = 0usize;
    let mut saved_row = 0usize;
    let mut pending_kitty: Option<Placement> = None;

    while at < bytes.len() {
        if bytes[at] == b'\n' {
            row = row.saturating_add(1);
            bottom = bottom.max(row.saturating_add(1));
            at += 1;
        } else if bytes[at] == 0x1b && bytes.get(at + 1) == Some(&b'_')
            && bytes.get(at + 2) == Some(&b'G')
        {
            let Some((end, next)) = string_end(bytes, at + 3, true) else {
                break;
            };
            let (command, more) = kitty_command(&output[at + 3..end]);
            if more {
                if command.is_some() {
                    pending_kitty = command;
                }
            } else if let Some(command) = command.or(pending_kitty) {
                // Only the last chunk of a transfer actually places the image.
                let height = command
                    .rows
                    .or(pending_kitty.and_then(|p| p.rows))
                    .unwrap_or(geometry.rows);
                bottom = bottom.max(row.saturating_add(height));
                if command.move_cursor {
                    row = row.saturating_add(height);
                }
                pending_kitty = None;
            }
            at = next;
        } else if bytes[at] == 0x1b && bytes.get(at + 1) == Some(&b'P') {
            if bytes[at + 2..].starts_with(b"tmux;") {
                let Some((inner, next)) = tmux_payload(bytes, at + 7) else {
                    break;
                };
                let height = measure_overlay(&inner, geometry);
                bottom = bottom.max(row.saturating_add(height));
                // Treat an opaque passthrough as advancing by its full extent.
                // This can overreserve when its graphic uses kitty's C=1.
                row = row.saturating_add(height);
                at = next;
                continue;
            }
            let Some((end, next)) = string_end(bytes, at + 2, false) else {
                break;
            };
            if let Some(pixels) = sixel_pixels(&output[at + 2..end]) {
                let height = geometry.sixel_rows(pixels);
                bottom = bottom.max(row.saturating_add(height));
                row = row.saturating_add(height); // sixel scrolling mode
            } else if output[at + 2..end].contains("_G") {
                // Other multiplexers may put a kitty image inside an opaque
                // DCS. Its placement size cannot safely be read here.
                bottom = bottom.max(row.saturating_add(geometry.rows));
                row = row.saturating_add(geometry.rows);
            }
            at = next;
        } else if bytes[at] == 0x1b && bytes.get(at + 1) == Some(&b'[') {
            let Some(offset) = bytes[at + 2..]
                .iter()
                .position(|&b| (0x40..=0x7e).contains(&b))
            else {
                break;
            };
            let end = at + 2 + offset;
            let count = output[at + 2..end]
                .split(';').next().and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1).max(1);
            match bytes[end] {
                b'A' | b'F' => row = row.saturating_sub(count),
                b'B' | b'E' => row = row.saturating_add(count),
                b's' => saved_row = row,
                b'u' => row = saved_row,
                _ => {}
            }
            at = end + 1;
        } else if bytes[at] == 0x1b && bytes.get(at + 1) == Some(&b']') {
            // OSC data is not visible text and may contain newlines.
            if let Some((end, next)) = string_end(bytes, at + 2, true) {
                if output[at + 2..end].starts_with("1337;File=") {
                    // iTerm-style images do not expose a reliable row count
                    // without interpreting their optional sizing parameters.
                    bottom = bottom.max(row.saturating_add(geometry.rows));
                    row = row.saturating_add(geometry.rows);
                }
                at = next;
            } else {
                break;
            }
        } else if bytes[at] == 0x1b {
            match bytes.get(at + 1) {
                Some(&b'7') => saved_row = row,
                Some(&b'8') => row = saved_row,
                Some(&b'D') | Some(&b'E') => row = row.saturating_add(1),
                _ => {}
            }
            at += if at + 1 < bytes.len() { 2 } else { 1 };
        } else {
            // OtterHelper turns off automatic wrapping while drawing overlays.
            if bytes[at] != b'\r' {
                bottom = bottom.max(row.saturating_add(1));
            }
            at += 1;
        }
    }
    bottom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(cell_px: Option<usize>) -> Geometry {
        Geometry { rows: 24, cell_px }
    }

    #[test]
    fn text_and_explicit_newlines() {
        assert_eq!(measure_overlay("", geometry(None)), 0);
        assert_eq!(measure_overlay("one\ntwo", geometry(None)), 2);
        assert_eq!(measure_overlay("one\n", geometry(None)), 2);
    }

    #[test]
    fn kitty_chunks_reuse_image_ids_without_summing_their_heights() {
        let image = "\x1b_Ga=T,i=7,r=8,m=1;QUJD\x1b\\\x1b_Gm=0;RA==\x1b\\";
        assert_eq!(measure_overlay(image, geometry(None)), 8);
        assert_eq!(measure_overlay(
            &format!("{image}\x1b[8A\x1b_Ga=p,i=7,r=8,C=1\x1b\\"),
            geometry(None)), 8);
        assert_eq!(measure_overlay(&format!("{image}\nmore"), geometry(None)), 10);
        assert_eq!(measure_overlay("\x1b_Ga=d,d=a,r=20\x1b\\", geometry(None)), 0);
        assert_eq!(measure_overlay("\x1b_Ga=p,P=7,r=2\x1b\\", geometry(None)), 24);
        assert_eq!(measure_overlay("\x1b_Ga=T,r=5,U=1;x\x1b\\", geometry(None)), 0);
    }

    #[test]
    fn sixel_raster_and_no_pixel_geometry() {
        let image = "\x1bPq\"1;1;12;41#0!30~-$~\x1b\\";
        assert_eq!(measure_overlay(image, geometry(Some(10))), 5);
        assert_eq!(measure_overlay(image, geometry(Some(20))), 3);
        assert_eq!(measure_overlay(image, geometry(None)), 24);
        assert_eq!(measure_overlay(&format!("top\n{image}"), geometry(Some(10))), 6);
        assert_eq!(measure_overlay("\x1bPq~~-~~\x1b\\", geometry(Some(10))), 2);
        assert_eq!(measure_overlay("\x1bP1;2pfoo\x1b\\", geometry(Some(10))), 0);
    }

    #[test]
    fn tmux_graphics_passthrough() {
        let wrapped = "\x1bPtmux;\x1b\x1b_Ga=T,r=4;AAAA\x1b\x1b\\\x1b\\";
        assert_eq!(measure_overlay(wrapped, geometry(None)), 4);
        assert_eq!(
            measure_overlay("\x1b]1337;File=inline=1:AAAA\x07", geometry(None)),
            24
        );
    }
}
